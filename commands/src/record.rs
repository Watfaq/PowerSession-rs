extern crate terminal;

use serde::Serialize;
use std::sync::mpsc::{channel, RecvError, Sender};
use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle, RawHandle},
    thread,
    time::SystemTime,
};
use terminal::{Terminal, WindowsTerminal};

#[derive(Serialize)]
struct RecordHeader {
    version: u8,
    width: u32,
    height: u32,
    timestamp: u64,
    #[serde(rename = "env")]
    environment: HashMap<&'static str, String>,
}

pub struct Record {
    output_writer: Box<dyn Write + Send + Sync>,
    env: HashMap<&'static str, String>,
    command: String,
    terminal: WindowsTerminal,
}

impl Record {
    pub fn new(
        filename: String,
        mut env: Option<HashMap<&'static str, String>>,
        command: String,
    ) -> Self {
        let cwd = std::env::current_dir().unwrap();
        Record {
            output_writer: Box::new(File::create(filename).expect("Can't create file")),
            env: env.get_or_insert(HashMap::new()).clone(),
            command: command,
            terminal: WindowsTerminal::new(cwd.to_str().unwrap().to_string()),
        }
    }
    pub fn execute(&mut self) {
        self.env.insert("POWERSESSION_RECORDING", "1".to_owned());
        self.env.insert("SHELL", "powershell.exe".to_owned());
        let term: String = env::var("TERMINAL_EMULATOR").unwrap_or("UnKnown".to_string());
        self.env.insert("TERM", term);

        self.record();
    }

    fn record(&mut self) {
        let header = RecordHeader {
            version: 2,
            width: 10,
            height: 10,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("check your machine time")
                .as_secs(),
            environment: self.env.clone(),
        };
        self.output_writer
            .write(serde_json::to_string(&header).unwrap().as_bytes());

        let (stdin_tx, stdin_rx) = channel::<u8>();
        let (stdout_tx, stdout_rx) = channel::<u8>();
        thread::spawn(move || loop {
            let mut stdin = std::io::stdin();
            let mut buf = [0, 1];
            let rv = stdin.read(&mut buf);
            match rv {
                Ok(n) if n > 0 => {
                    stdin_tx.send(buf[0]);
                }
                _ => break,
            }
        });
        let mut stdout = std::io::stdout();

        thread::spawn(move || loop {
            let rv = stdout_rx.recv();

            match rv {
                Ok(byte) => {
                    stdout.write(&[byte]);
                }
                Err(err) => {
                    println!("{}", err);
                    break;
                }
            }
        });
        self.terminal.attach_stdout(stdout_tx.clone());
        self.terminal.attach_stdin(stdin_rx);
        self.terminal.run(&self.command);
    }
}
