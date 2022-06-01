use crate::types::{LineItem, RecordHeader, SessionLine};


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
    line_iter: io::Lines<io::BufReader<File>>,
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
            println!("session with name {} does not exist", filename);
            exit(1);
        }

        let mut line_iter = read_lines(filename).unwrap();
        let header_line = line_iter.next().unwrap();
        let header: RecordHeader = serde_json::from_str(header_line.unwrap().as_str()).unwrap();
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
}

impl Play {
    pub fn new(filename: String) -> Self {
        Play {
            session: Session::new(&filename),
        }
    }

    pub fn execute(self) {
        let cond = Condvar::new();
        let g = Mutex::new(false);
        #[allow(unused_must_use)]
        for stdout in self.session.stdout_relative_time_iter() {
            cond.wait_timeout(g.lock().unwrap(), Duration::from_secs_f64(stdout.timestamp))
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

    #[test]
    fn test_play() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("testdata/play.txt");

        let play = Play::new(d.as_path().to_str().unwrap().to_owned());
        play.execute();
    }
}
