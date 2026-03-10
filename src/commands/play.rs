use crate::commands::types::{LineItem, RecordHeader, SessionLine};

use std::fs::File;
use std::io;
use std::io::{BufRead, Write};

use std::path::Path;
use std::process::exit;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

struct Session {
    #[allow(dead_code)]
    header: RecordHeader,
    line_iter: io::Lines<Box<dyn BufRead>>,
}

struct StdoutIter(Session);

impl Iterator for StdoutIter {
    type Item = SessionLine;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.line_iter.next().map(|line| {
            let content = line.unwrap();
            let line_data: Vec<LineItem> = serde_json::from_str(&content).unwrap();
            if line_data.len() != 3 {
                panic!("invalid record data");
            }

            SessionLine {
                timestamp: match &line_data[0] {
                    LineItem::F64(ts) => ts.clone(),
                    _ => {
                        panic!("corrupt record");
                    }
                },
                stdout: match &line_data[1] {
                    LineItem::String(flag) => flag == "o",
                    _ => {
                        panic!("corrupt record");
                    }
                },
                content: match &line_data[2] {
                    LineItem::String(line) => line.clone(),
                    _ => {
                        panic!("corrupt record");
                    }
                },
            }
        })
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

impl Session {
    fn new(source: &str) -> Self {
        if is_url(source) {
            Self::from_url(source)
        } else {
            Self::from_file(source)
        }
    }

    fn from_file(filename: &str) -> Self {
        if !Path::new(filename).exists() {
            eprintln!("File {} does not exist", filename);
            exit(1);
        }

        let file = File::open(filename).unwrap();
        Self::from_reader(Box::new(io::BufReader::new(file)))
    }

    fn from_url(url: &str) -> Self {
        let url = normalize_url(url);
        let response = reqwest::blocking::get(&url).unwrap_or_else(|e| {
            eprintln!("Failed to fetch URL {}: {}", url, e);
            exit(1);
        });

        if !response.status().is_success() {
            eprintln!("Failed to fetch URL {}: HTTP {}", url, response.status());
            exit(1);
        }

        Self::from_reader(Box::new(io::BufReader::new(response)))
    }

    fn from_reader(reader: Box<dyn BufRead>) -> Self {
        let mut line_iter = reader.lines();
        let header_line = line_iter
            .next()
            .unwrap_or_else(|| {
                eprintln!("error: session file is empty");
                exit(1);
            })
            .unwrap_or_else(|e| {
                eprintln!("error: failed to read session header: {}", e);
                exit(1);
            });
        let header: RecordHeader = serde_json::from_str(&header_line).unwrap_or_else(|e| {
            eprintln!("error: session file has an invalid header: {}", e);
            exit(1);
        });
        Session { header, line_iter }
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
