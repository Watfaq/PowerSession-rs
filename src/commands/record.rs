use std::path::Path;
use std::process::exit;

use log::{error, trace};
use std::sync::mpsc::channel;
use std::sync::Mutex;
use std::time::SystemTime;
use std::{
    collections::HashMap,
    env, fs,
    fs::File,
    io::{Read, Write},
    sync::Arc,
    thread,
};

use crate::commands::types::LineItem;
use crate::commands::types::RecordHeader;
use crate::terminal::{Terminal, WindowsTerminal};

pub struct Record {
    output_writer: Arc<Mutex<Box<dyn Write + Send + Sync>>>,
    filename: String,
    env: HashMap<String, String>,
    command: String,
    #[cfg(windows)]
    terminal: WindowsTerminal,
}

impl Record {
    pub fn new(
        filename: String,
        env: Option<HashMap<String, String>>,
        command: Option<String>,
        overwrite: bool,
    ) -> Self {
        if Path::new(&filename).exists() {
            println!("session with name `{}` exists", filename);
            if overwrite {
                println!("overwrite flag provided. deleting the existing session");
                fs::remove_file(&filename).unwrap();
            } else {
                println!("use -f to overwrite");
                exit(1);
            }
        }

        Record {
            output_writer: Arc::new(Mutex::new(Box::new(File::create(&filename).unwrap()))),
            filename,
            env: env.unwrap_or_default(),
            command: command
                .unwrap_or_else(|| env::var("SHELL").unwrap_or("powershell.exe".to_owned())),
            terminal: WindowsTerminal::new(None),
        }
    }
    pub fn execute(&mut self) {
        self.env.insert(
            "SHELL".to_string(),
            env::var("SHELL").unwrap_or("powershell.exe".to_owned()),
        );

        let term = match env::var("WT_SESSION") {
            Ok(sess) if sess.len() > 0 => Some("windows-terminal".to_owned()),
            _ => env::var("TERM").ok(),
        };
        if let Some(term) = term {
            self.env.insert("TERM".to_string(), term);
        }

        self.record();
    }

    fn record(&mut self) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("check your machine time");

        let record_start_time = now.as_secs() as f64 + now.subsec_nanos() as f64 * 1e-9;

        let header = RecordHeader {
            version: 2,
            width: self.terminal.width,
            height: self.terminal.height,
            timestamp: record_start_time as u64,
            environment: self.env.clone(),
        };

        self.output_writer
            .lock()
            .unwrap()
            .write((serde_json::to_string(&header).unwrap() + "\n").as_bytes())
            .unwrap();

        let (stdin_tx, stdin_rx) = channel::<(Arc<[u8]>, usize)>();
        let (stdout_tx, stdout_rx) = channel::<(Arc<[u8]>, usize)>();

        thread::spawn(move || loop {
            let stdin = std::io::stdin();
            let mut handle = stdin.lock();
            let mut buf = [0; 10];
            let rv = handle.read(&mut buf);
            match rv {
                Ok(n) if n > 0 => {
                    stdin_tx.send((Arc::from(buf), n)).unwrap();
                }
                _ => {
                    panic!("pty stdin closed");
                }
            }
        });

        let output_writer = self.output_writer.clone();
        let filename = self.filename.clone();

        thread::spawn(move || loop {
            let mut stdout = std::io::stdout();

            let rv = stdout_rx.recv();
            match rv {
                Ok((buf, len)) => {
                    if len == 0 {
                        trace!("stdout received close indicator");
                        println!("Record finished. Result saved to file {}", filename);
                        break;
                    }

                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .expect("check your machine time");

                    let ts =
                        now.as_secs() as f64 + now.subsec_nanos() as f64 * 1e-9 - record_start_time;
                    let chars = String::from_utf8(buf[..len].to_vec()).unwrap();
                    let data = vec![
                        LineItem::F64(ts),
                        LineItem::String("o".to_string()),
                        LineItem::String(chars),
                    ];
                    let line = serde_json::to_string(&data).unwrap() + "\n";
                    output_writer
                        .lock()
                        .unwrap()
                        .write(line.as_bytes())
                        .unwrap();

                    stdout.write(&buf[..len]).expect("failed to write stdout");
                    stdout.flush().expect("failed to flush stdout");
                }

                Err(err) => {
                    error!("reading stdout: {}", err.to_string());
                    break;
                }
            }
        });

        self.terminal.attach_stdin(stdin_rx);
        self.terminal.attach_stdout(stdout_tx);
        self.terminal.run(&self.command).unwrap();
    }
}
