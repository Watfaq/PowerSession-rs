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

#[cfg(windows)]
use windows::Win32::{
    Foundation::HANDLE,
    System::Console::{GetStdHandle, WriteConsoleW, STD_OUTPUT_HANDLE},
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
            Ok(sess) if sess.len() > 0 => Some("ms-terminal".to_owned()),
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

        let (stdin_tx, stdin_rx) = channel::<(Vec<u8>, usize)>();
        let (stdout_tx, stdout_rx) = channel::<(Vec<u8>, usize)>();

        thread::spawn(move || {
            loop {
                let stdin = std::io::stdin();
                let mut handle = stdin.lock();
                let mut buf = [0; 10];
                let rv = handle.read(&mut buf);
                match rv {
                    Ok(n) if n > 0 => {
                        stdin_tx.send((buf.to_vec(), n)).unwrap();
                    }
                    _ => {
                        panic!("pty stdin closed");
                    }
                }
            }
        });

        let output_writer = self.output_writer.clone();
        let filename = self.filename.clone();

        thread::spawn(move || {
            // Use raw Windows handle to write bytes directly, bypassing Rust's UTF-8 validation
            // which fails on Windows console mode with non-UTF-8 sequences
            #[cfg(windows)]
            let stdout_handle: HANDLE =
                unsafe { GetStdHandle(STD_OUTPUT_HANDLE).expect("failed to get stdout handle") };

            // Buffer for incomplete UTF-8 sequences split across chunk boundaries
            let mut pending_bytes: Vec<u8> = Vec::new();

            loop {
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

                        let ts = now.as_secs() as f64 + now.subsec_nanos() as f64 * 1e-9
                            - record_start_time;

                        // Combine pending bytes with new data
                        pending_bytes.extend_from_slice(&buf[..len]);

                        // Find the last valid UTF-8 boundary
                        let valid_up_to = match std::str::from_utf8(&pending_bytes) {
                            Ok(_) => pending_bytes.len(),
                            Err(e) => e.valid_up_to(),
                        };

                        // Only process complete UTF-8 sequences
                        if valid_up_to > 0 {
                            // Safe: we just validated these bytes are valid UTF-8
                            let chars = std::str::from_utf8(&pending_bytes[..valid_up_to]).unwrap();

                            // https://github.com/asciinema/asciinema/blob/5a385765f050e04523c9d74fbf98d5afaa2deff0/asciinema/asciicast/v2.py#L119
                            let data = vec![
                                LineItem::F64(ts),
                                LineItem::String("o".to_string()),
                                LineItem::String(chars.to_string()),
                            ];
                            let line = serde_json::to_string(&data).unwrap() + "\n";
                            output_writer
                                .lock()
                                .unwrap()
                                .write(line.as_bytes())
                                .unwrap();

                            // Write to console using WriteConsoleW for proper Unicode support
                            #[cfg(windows)]
                            unsafe {
                                let utf16: Vec<u16> = chars.encode_utf16().collect();
                                WriteConsoleW(stdout_handle, &utf16, None, None)
                                    .expect("failed to write stdout");
                            }
                        }

                        // Keep incomplete bytes for next iteration
                        pending_bytes.drain(..valid_up_to);
                    }

                    Err(err) => {
                        error!("reading stdout: {}", err.to_string());
                        break;
                    }
                }
            }
        });

        self.terminal.attach_stdin(stdin_rx);
        self.terminal.attach_stdout(stdout_tx);
        self.terminal.run(&self.command).unwrap();
    }
}
