use crate::commands::types::{LineItem, RecordHeader, SessionLine, V1Recording};

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::{BufRead, Write};

use std::path::Path;
use std::process::exit;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

enum SessionLineSource {
    Lines(io::Lines<Box<dyn BufRead>>),
    Vec(std::vec::IntoIter<SessionLine>),
}

struct Session {
    #[allow(dead_code)]
    header: RecordHeader,
    line_iter: SessionLineSource,
}

struct StdoutIter(Session);

impl Iterator for StdoutIter {
    type Item = SessionLine;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let event = match &mut self.0.line_iter {
                SessionLineSource::Vec(iter) => iter.next(),
                SessionLineSource::Lines(iter) => iter.next().map(|line| {
                    let content = match line {
                        Ok(l) => l,
                        Err(e) => {
                            eprintln!("error reading session data: {}", e);
                            exit(1);
                        }
                    };
                    let line_data: Vec<LineItem> = match serde_json::from_str(&content) {
                        Ok(data) => data,
                        Err(e) => {
                            eprintln!("corrupt record data: {}", e);
                            exit(1);
                        }
                    };
                    if line_data.len() != 3 {
                        eprintln!("corrupt record: expected 3 fields, got {}", line_data.len());
                        exit(1);
                    }

                    SessionLine {
                        timestamp: match &line_data[0] {
                            LineItem::F64(ts) => ts.clone(),
                            _ => {
                                eprintln!("corrupt record: expected timestamp as number");
                                exit(1);
                            }
                        },
                        stdout: match &line_data[1] {
                            LineItem::String(flag) => flag == "o",
                            _ => {
                                eprintln!("corrupt record: expected event type as string");
                                exit(1);
                            }
                        },
                        content: match &line_data[2] {
                            LineItem::String(line) => line.clone(),
                            _ => {
                                eprintln!("corrupt record: expected content as string");
                                exit(1);
                            }
                        },
                    }
                }),
            };
            match event {
                // Only yield output ("o") events; skip input ("i") and any other event types.
                Some(line) if line.stdout => return Some(line),
                Some(_) => continue,
                None => return None,
            }
        }
    }
}

struct StdoutRelativeTimeIter(StdoutIter, f64);

impl Iterator for StdoutRelativeTimeIter {
    type Item = SessionLine;

    fn next(&mut self) -> Option<Self::Item> {
        let prev_timestamp = self.1;

        self.0.next().map(|line| {
            let rv = SessionLine {
                timestamp: match prev_timestamp {
                    x if x == 0.0 => 0.0, // first line, start right away
                    _ => line.timestamp - prev_timestamp,
                },
                content: line.content,
                stdout: line.stdout,
            };
            self.1 = line.timestamp;
            rv
        })
    }
}

fn is_url(input: &str) -> bool {
    input.starts_with("http://") || input.starts_with("https://")
}

/// Normalize an asciinema.org recording URL to its raw `.cast` download URL.
/// For example, `https://asciinema.org/a/abc123` becomes
/// `https://asciinema.org/a/abc123.cast`.
/// Query strings and fragments are preserved (e.g. `?t=10` stays after `.cast`).
fn normalize_url(url: &str) -> String {
    // Split off fragment, preserving the leading '#'
    let (before_fragment, fragment) = match url.find('#') {
        Some(idx) => (&url[..idx], &url[idx..]),
        None => (url, ""),
    };

    // Split off query, preserving the leading '?'
    let (mut main, query) = match before_fragment.find('?') {
        Some(idx) => (&before_fragment[..idx], &before_fragment[idx..]),
        None => (before_fragment, ""),
    };

    // Only normalize asciinema recording URLs
    if main.contains("asciinema.org/a/") {
        // Remove a trailing slash from the path, if present
        if main.ends_with('/') {
            main = &main[..main.len() - 1];
        }

        let mut normalized = main.to_string();
        if !normalized.ends_with(".cast") {
            normalized.push_str(".cast");
        }

        // Reattach query and fragment in their original order
        normalized.push_str(query);
        normalized.push_str(fragment);
        normalized
    } else {
        // Non-asciinema URLs are returned unchanged
        url.to_string()
    }
}

/// Parse a session from a buffered reader, detecting v2 or v1 format automatically.
/// The `source_name` is used only in error messages.
fn parse_reader(reader: Box<dyn BufRead>, source_name: &str) -> Session {
    let mut line_iter: io::Lines<Box<dyn BufRead>> = reader.lines();

    let first_line = match line_iter.next() {
        Some(Ok(line)) => line,
        Some(Err(e)) => {
            eprintln!("error reading '{}': {}", source_name, e);
            exit(1);
        }
        None => {
            eprintln!("'{}': file is empty", source_name);
            exit(1);
        }
    };

    if let Ok(header) = serde_json::from_str::<RecordHeader>(&first_line) {
        // v2 format: header on first line, events stream on subsequent lines
        Session {
            header,
            line_iter: SessionLineSource::Lines(line_iter),
        }
    } else {
        // Try v1 format: entire content is a single JSON object.
        // Collect remaining lines from the already-opened iterator.
        let mut file_content = first_line;
        for line in line_iter {
            file_content.push('\n');
            match line {
                Ok(l) => file_content.push_str(&l),
                Err(e) => {
                    eprintln!("error reading '{}': {}", source_name, e);
                    exit(1);
                }
            }
        }
        match serde_json::from_str::<V1Recording>(&file_content) {
            Ok(recording) if recording.version == 1 => {
                let header = RecordHeader {
                    version: recording.version,
                    width: recording.width,
                    height: recording.height,
                    timestamp: 0,
                    environment: HashMap::new(),
                };

                let mut absolute_time: f64 = 0.0;
                let events: Vec<SessionLine> = recording
                    .stdout
                    .into_iter()
                    .map(|(delay, text)| {
                        absolute_time += delay;
                        SessionLine {
                            timestamp: absolute_time,
                            stdout: true,
                            content: text,
                        }
                    })
                    .collect();

                Session {
                    header,
                    line_iter: SessionLineSource::Vec(events.into_iter()),
                }
            }
            Ok(recording) => {
                eprintln!(
                    "'{}': unsupported file format version {}",
                    source_name, recording.version
                );
                exit(1);
            }
            Err(e) => {
                eprintln!(
                    "'{}': unsupported or corrupt session file: {}",
                    source_name, e
                );
                exit(1);
            }
        }
    }
}

impl Session {
    fn new(source: &str) -> Self {
        if is_url(source) { Self::from_url(source) } else { Self::from_file(source) }
    }

    fn from_file(filename: &str) -> Self {
        if !Path::new(filename).exists() {
            eprintln!("session with name {} does not exist", filename);
            exit(1);
        }

        let file = match File::open(filename) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("error opening '{}': {}", filename, e);
                exit(1);
            }
        };
        parse_reader(Box::new(io::BufReader::new(file)), filename)
    }

    fn from_url(url: &str) -> Self {
        let url = normalize_url(url);
        let response = reqwest::blocking::get(&url).unwrap_or_else(|e| {
            eprintln!("failed to fetch URL {}: {}", url, e);
            exit(1);
        });

        if !response.status().is_success() {
            eprintln!("failed to fetch URL {}: HTTP {}", url, response.status());
            exit(1);
        }

        parse_reader(Box::new(io::BufReader::new(response)), &url)
    }

    fn stdout_iter(self) -> StdoutIter {
        StdoutIter(self)
    }

    fn stdout_relative_time_iter(self) -> StdoutRelativeTimeIter {
        StdoutRelativeTimeIter(self.stdout_iter(), 0.0)
    }
}

pub struct Play {
    session: Session,
    idle_time_limit: Option<f64>,
    speed: f64,
}

impl Play {
    pub fn new(filename: String, idle_time_limit: Option<f64>, speed: f64) -> Self {
        Play {
            session: Session::new(&filename),
            idle_time_limit,
            speed,
        }
    }

    pub fn execute(self) {
        let cond = Condvar::new();
        let g = Mutex::new(false);
        #[allow(unused_must_use)]
        for stdout in self.session.stdout_relative_time_iter() {
            let mut delay = stdout.timestamp;
            if let Some(limit) = self.idle_time_limit {
                delay = delay.min(limit);
            }
            delay /= self.speed;
            cond.wait_timeout(g.lock().unwrap(), Duration::from_secs_f64(delay))
                .unwrap();
            io::stdout().write_all(stdout.content.as_bytes()).unwrap();
            io::stdout().flush().unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Play;
    use std::path::PathBuf;

    use super::{is_url, normalize_url};

    fn test_data_path() -> String {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("testdata/play.txt");
        d.as_path().to_str().unwrap().to_owned()
    }

    fn test_data_v1_path() -> String {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("testdata/play_v1.json");
        d.as_path().to_str().unwrap().to_owned()
    }

    fn test_data_with_stdin_path() -> String {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("testdata/play_with_stdin.txt");
        d.as_path().to_str().unwrap().to_owned()
    }

    #[test]
    fn test_play() {
        let play = Play::new(test_data_path(), None, 1.0);
        play.execute();
    }

    #[test]
    fn test_play_with_speed() {
        let play = Play::new(test_data_path(), None, 2.0);
        play.execute();
    }

    #[test]
    fn test_play_with_idle_time_limit() {
        let play = Play::new(test_data_path(), Some(0.5), 1.0);
        play.execute();
    }

    #[test]
    fn test_play_with_speed_and_idle_time_limit() {
        let play = Play::new(test_data_path(), Some(0.5), 2.0);
        play.execute();
    }

    #[test]
    fn test_play_v1_format() {
        let play = Play::new(test_data_v1_path(), None, 1.0);
        play.execute();
    }

    #[test]
    fn test_play_v1_format_with_speed() {
        let play = Play::new(test_data_v1_path(), None, 2.0);
        play.execute();
    }

    #[test]
    fn test_play_v1_format_with_idle_time_limit() {
        let play = Play::new(test_data_v1_path(), Some(0.5), 1.0);
        play.execute();
    }

    /// Playback of a cast that contains "i" (stdin) events alongside "o" (stdout)
    /// events should silently skip the input events and only render output events.
    #[test]
    fn test_play_skips_stdin_events() {
        let play = Play::new(test_data_with_stdin_path(), None, 1.0);
        play.execute();
    }

    /// Timing deltas must be computed only between consecutive "o" events, not
    /// relative to interleaved "i" events.
    #[test]
    fn test_play_stdin_events_do_not_affect_timing() {
        let play = Play::new(test_data_with_stdin_path(), Some(0.5), 2.0);
        play.execute();
    }

    #[test]
    fn test_is_url() {
        assert!(is_url("https://asciinema.org/a/abc123"));
        assert!(is_url("http://example.com/recording.cast"));
        assert!(!is_url("my_recording.cast"));
        assert!(!is_url("/path/to/recording.cast"));
        assert!(!is_url("C:\\recordings\\session.cast"));
    }

    #[test]
    fn test_normalize_url_asciinema() {
        // Basic recording URL
        assert_eq!(
            normalize_url("https://asciinema.org/a/abc123"),
            "https://asciinema.org/a/abc123.cast"
        );
        // Already has .cast – should not double-append
        assert_eq!(
            normalize_url("https://asciinema.org/a/abc123.cast"),
            "https://asciinema.org/a/abc123.cast"
        );
        // Non-asciinema URL – should be returned unchanged
        assert_eq!(
            normalize_url("https://example.com/recording.cast"),
            "https://example.com/recording.cast"
        );
        // URL with query string – .cast inserted before '?'
        assert_eq!(
            normalize_url("https://asciinema.org/a/abc123?t=10"),
            "https://asciinema.org/a/abc123.cast?t=10"
        );
        // URL with trailing slash – slash stripped before appending .cast
        assert_eq!(
            normalize_url("https://asciinema.org/a/abc123/"),
            "https://asciinema.org/a/abc123.cast"
        );
        // URL with fragment – .cast inserted before '#'
        assert_eq!(
            normalize_url("https://asciinema.org/a/abc123#intro"),
            "https://asciinema.org/a/abc123.cast#intro"
        );
        // URL with both query string and fragment
        assert_eq!(
            normalize_url("https://asciinema.org/a/abc123?t=10#intro"),
            "https://asciinema.org/a/abc123.cast?t=10#intro"
        );
    }
}
