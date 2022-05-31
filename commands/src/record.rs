extern crate terminal;

use std::sync::mpsc::channel;
use std::time::SystemTime;
use std::{
    collections::HashMap,
    env,
    fs::File,
    io,
    io::{Read, Write},
    sync::Arc,
    thread,
};

use serde::Serialize;

use terminal::{Terminal, WindowsTerminal};

trait OutputWriter {
    fn write(&mut self, data: String) -> io::Result<()>;
}

struct FileOutputWriter {
    fd: File,
}

impl FileOutputWriter {
    fn new(file: String) -> Self {
        FileOutputWriter {
            fd: File::create(file).expect("unable to open file"),
        }
    }
}

impl OutputWriter for FileOutputWriter {
    fn write(&mut self, data: String) -> io::Result<()> {
        self.fd.write_all(data.as_bytes())
    }
}

#[derive(Serialize)]
struct RecordHeader {
    version: u8,
    width: i16,
    height: i16,
    timestamp: u64,
    #[serde(rename = "env")]
    environment: HashMap<String, String>,
}

pub struct Record {
    output_writer: Box<dyn OutputWriter>,
    env: HashMap<String, String>,
    command: String,
    #[cfg(windows)]
    terminal: WindowsTerminal,
}

#[derive(Debug, Serialize)]
enum LineItem {
    String(String),
    U64(u64),
}

impl Record {
    pub fn new(
        filename: String,
        mut env: Option<HashMap<String, String>>,
        command: String,
    ) -> Self {
        Record {
            output_writer: Box::new(FileOutputWriter::new(filename)),
            env: env.get_or_insert(HashMap::new()).clone(), // this clone() looks wrong??
            command,
            terminal: WindowsTerminal::new(None),
        }
    }
    pub fn execute(&mut self) {
        self.env
            .insert("POWERSESSION_RECORDING".to_string(), "1".to_string());
        self.env
            .insert("SHELL".to_string(), "powershell.exe".to_string());
        let term: String = env::var("TERMINAL_EMULATOR").unwrap_or("UnKnown".to_string());
        self.env.insert("TERM".to_string(), term);

        self.record();
    }

    fn record(&mut self) {
        let record_start_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("check your machine time")
            .as_secs();

        let header = RecordHeader {
            version: 2,
            width: self.terminal.width,
            height: self.terminal.height,
            timestamp: record_start_time,
            environment: self.env.clone(),
        };

        self.output_writer
            .write(serde_json::to_string(&header).unwrap())
            .unwrap();

        let (stdin_tx, stdin_rx) = channel::<(Arc<[u8; 1]>, usize)>();
        let (stdout_tx, stdout_rx) = channel::<(Arc<[u8; 1024]>, usize)>();

        thread::spawn(move || loop {
            let stdin = std::io::stdin();
            let mut handle = stdin.lock();
            let mut buf = [0; 1];
            let rv = handle.read(&mut buf);
            match rv {
                Ok(n) if n > 0 => {
                    stdin_tx.send((Arc::from(buf), n)).unwrap();
                }
                _ => {
                    println!("pty stdin closed");
                    break;
                }
            }
        });

        thread::spawn(move || loop {
            let mut stdout = std::io::stdout();

            let rv = stdout_rx.recv();
            match rv {
                Ok((buf, len)) => {
                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .expect("check your machine time")
                        .as_secs();
                    let ts = now - record_start_time;
                    let chars = String::from_utf8(buf[..len].to_vec()).unwrap();
                    let mut data = vec![
                        LineItem::U64(ts),
                        LineItem::String("o".to_string()),
                        LineItem::String(chars),
                    ];
                    let line = serde_json::to_string(&data).unwrap();
                    self.output_writer.write(line).unwrap();

                    stdout.write(&buf[..len]).expect("failed to write stdout");
                    stdout.flush().expect("failed to flush stdout");
                }
                Err(err) => {
                    // the stdout_rx closed, mostly due to process exited.
                    break;
                }
            }
        });

        self.terminal.attach_stdin(stdin_rx);
        self.terminal.attach_stdout(stdout_tx);
        self.terminal.run(&self.command).unwrap();
    }
}
