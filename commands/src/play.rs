use crate::types::RecordHeader;
use std::cmp::Ordering;
use std::convert::Infallible;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::path::Path;

struct Session {
    header: RecordHeader,
    line_iter: io::Lines<io::BufReader<File>>,
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
            panic!("file {} not exist", filename);
        }

        let mut line_iter = read_lines(filename).unwrap();
        let header_line = line_iter.next().unwrap();
        let header: RecordHeader = serde_json::from_str(header_line.unwrap().as_str()).unwrap();
        Session { header, line_iter }
    }

    fn execute(&self) {}
}
