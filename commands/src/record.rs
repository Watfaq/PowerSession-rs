use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{BufRead, Read, Write},
    thread,
};

pub trait Terminal {
    fn run(&self, command: &str);
    fn get_input_file(&self) -> File;
    fn get_output_file(&self) -> File;
}

struct RecordHeader {
    version: u8,
    width: u32,
    height: u32,
    timestamp: u64,
    env: HashMap<&'static str, String>,
}

pub struct Record<'a> {
    output_writer: File,
    env: Option<HashMap<&'a str, String>>,
    command: &'a str,
    terminal: &'a Box<dyn Terminal>,
}

impl<'a> Record<'a> {
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
            .get_input_file()
            .write(input.as_bytes())
            .expect("Can't write to Terminal");
    }

    fn record(&self) {
        let mut terminal_output = File::from(self.terminal.get_output_file());
        let mut output_receiver = self.output_writer.as_raw_fd();

        thread::spawn(|| loop {
            let mut buf = [0, 10];
            let rv = terminal_output.read(&mut buf);
            match rv {
                Ok(n) if n > 0 => {
                    output_receiver.write(&buf[..n]);
                }
                _ => break,
            }
        });
        self.terminal.run(self.command);
    }
}
