extern crate terminal;

use serde::Serialize;
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
struct RecordHeader<'a> {
    version: u8,
    width: u32,
    height: u32,
    timestamp: u64,
    #[serde(rename = "env")]
    environment: &'a HashMap<&'a str, String>,
}

pub struct Record<'a> {
    output_writer: RawHandle, // TODO: should be a platform independent
    env: Option<HashMap<&'a str, String>>,
    command: &'a str,
    terminal: Box<dyn Terminal + 'a>,
}

impl<'a> Record<'a> {
    pub fn new(filename: &'a str, env: Option<HashMap<&'a str, String>>, command: &'a str) -> Self {
        Record {
            output_writer: File::create(filename)
                .expect("Can't create file")
                .as_raw_handle(),
            env: env,
            command: command,
            terminal: WindowsTerminal::new(command),
        }
    }
    pub fn execute(&mut self) {
        let env = self.env.get_or_insert(HashMap::new());

        env.insert("POWERSESSION_RECORDING", "1".to_owned());
        env.insert("SHELL", "powershell.exe".to_owned());
        let term: String = env::var("TERMINAL_EMULATOR").unwrap_or("UnKnown".to_string());
        env.insert("TERM", term);

        self.record();
    }

    pub fn feed_input(&self, input: &str) {
        self.terminal
            .get_stdin()
            .write(input.as_bytes())
            .expect("Can't write to Terminal");
    }

    fn record(&self) {
        let header = RecordHeader {
            version: 2,
            width: 10,
            height: 10,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("check your machine time")
                .as_secs(),
            environment: self.env.as_ref().unwrap(),
        };
        unsafe {
            let mut terminal_output = self.terminal.get_stdout();
            let mut output_receiver = File::from_raw_handle(self.output_writer);

            output_receiver
                .write(serde_json::to_string(&header).unwrap().as_bytes())
                .expect("Can't write to output receiver");
            thread::spawn(move || loop {
                let mut buf = [0, 10];
                let rv = terminal_output.read(&mut buf);
                match rv {
                    Ok(n) if n > 0 => {
                        output_receiver
                            .write(&buf[..n])
                            .expect("Failed to write to output");
                    }
                    _ => break,
                }
            });
            self.terminal.run(self.command);
        }
    }
}
