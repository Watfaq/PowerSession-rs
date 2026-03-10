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
    File(io::Lines<io::BufReader<File>>),
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
        match &mut self.0.line_iter {
            SessionLineSource::Vec(iter) => iter.next(),
            SessionLineSource::File(iter) => iter.next().map(|line| {
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

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

impl Session {
    fn new(filename: &str) -> Self {
        if !Path::new(filename).exists() {
            eprintln!("session with name {} does not exist", filename);
            exit(1);
        }

        let mut line_iter = match read_lines(filename) {
            Ok(iter) => iter,
            Err(e) => {
                eprintln!("error opening '{}': {}", filename, e);
                exit(1);
            }
        };
        let first_line = match line_iter.next() {
            Some(Ok(line)) => line,
            Some(Err(e)) => {
                eprintln!("error reading '{}': {}", filename, e);
                exit(1);
            }
            None => {
                eprintln!("'{}': file is empty", filename);
                exit(1);
            }
        };

        if let Ok(header) = serde_json::from_str::<RecordHeader>(&first_line) {
            // v2 format: header on first line, events on subsequent lines
            Session {
                header,
                line_iter: SessionLineSource::File(line_iter),
            }
        } else {
            // Try v1 format: entire file is a single JSON object.
            // Collect remaining lines from the already-opened iterator to avoid re-reading the file.
            let mut file_content = first_line;
            for line in line_iter {
                file_content.push('\n');
                match line {
                    Ok(l) => file_content.push_str(&l),
                    Err(e) => {
                        eprintln!("error reading '{}': {}", filename, e);
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
                        filename, recording.version
                    );
                    exit(1);
                }
                Err(e) => {
                    eprintln!("'{}': unsupported or corrupt session file: {}", filename, e);
                    exit(1);
                }
            }
        }
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
}
