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
    io::Write,
    sync::Arc,
    thread,
};

#[cfg(windows)]
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::ReadFile,
    System::Console::{GetStdHandle, WriteConsoleW, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
};

use crate::commands::types::LineItem;
use crate::commands::types::RecordHeader;
#[cfg(windows)]
use crate::terminal::Terminal;
#[cfg(windows)]
use crate::terminal::WindowsTerminal;

pub struct Record {
    output_writer: Arc<Mutex<Box<dyn Write + Send + Sync>>>,
    filename: String,
    env: HashMap<String, String>,
    command: String,
    stdin: bool,
    #[cfg(windows)]
    terminal: WindowsTerminal,
}

impl Record {
    pub fn new(
        filename: String,
        env: Option<HashMap<String, String>>,
        command: Option<String>,
        overwrite: bool,
        stdin: bool,
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
            stdin,
            #[cfg(windows)]
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
            #[cfg(windows)]
            width: self.terminal.width,
            #[cfg(not(windows))]
            width: 80,
            #[cfg(windows)]
            height: self.terminal.height,
            #[cfg(not(windows))]
            height: 24,
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

        // Single channel for cast-file events.  Both the stdin and stdout threads
        // forward pre-formatted JSON lines here.  A dedicated writer thread drains
        // the channel and writes to the file, ensuring serialised, in-arrival-order
        // writes without competing mutex acquisitions between threads.
        //   Some(line) – write this line to the cast file
        //   None       – stdout closed; recording is done
        let (event_tx, event_rx) = channel::<Option<String>>();

        // On Windows, use ReadFile directly on the stdin handle instead of
        // std::io::stdin() (which uses ReadConsoleW internally). When raw mode is
        // active (ENABLE_LINE_INPUT and ENABLE_PROCESSED_INPUT both disabled),
        // ReadConsoleW silently drops ESC (0x1B), stripping the prefix from VT
        // sequences such as \x1bOP (F1) or \x1b[A (arrow up).  ReadFile reads raw
        // bytes without any ESC processing, so all key sequences are forwarded
        // intact.
        #[cfg(windows)]
        let stdin_handle: isize = unsafe {
            GetStdHandle(STD_INPUT_HANDLE)
                .expect("failed to get Windows stdin handle (STD_INPUT_HANDLE)")
                .0 as isize
        };

        let record_stdin = self.stdin;
        let stdin_event_tx = event_tx.clone();

        thread::spawn(move || {
            // Buffer for incomplete UTF-8 sequences split across read boundaries,
            // mirroring the pending_bytes approach used in the stdout thread.
            let mut pending_bytes: Vec<u8> = Vec::new();

            loop {
                let mut buf = [0u8; 10];

                #[cfg(windows)]
                let n = {
                    let mut n_read: u32 = 0;
                    let ok = unsafe {
                        ReadFile(
                            HANDLE(stdin_handle as _),
                            Some(&mut buf),
                            Some(&mut n_read),
                            None,
                        )
                        .is_ok()
                    };
                    if !ok {
                        panic!("ReadFile on stdin failed");
                    }
                    if n_read == 0 {
                        panic!("pty stdin closed");
                    }
                    n_read as usize
                };

                #[cfg(not(windows))]
                let n = {
                    use std::io::Read;
                    match std::io::stdin().lock().read(&mut buf) {
                        Ok(n) if n > 0 => n,
                        _ => panic!("pty stdin closed"),
                    }
                };

                if record_stdin {
                    pending_bytes.extend_from_slice(&buf[..n]);

                    let valid_up_to = match std::str::from_utf8(&pending_bytes) {
                        Ok(_) => pending_bytes.len(),
                        Err(e) => e.valid_up_to(),
                    };

                    if valid_up_to > 0 {
                        let chars =
                            std::str::from_utf8(&pending_bytes[..valid_up_to]).unwrap();
                        let now = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .expect("check your machine time");
                        let ts = now.as_secs() as f64 + now.subsec_nanos() as f64 * 1e-9
                            - record_start_time;
                        let data = vec![
                            LineItem::F64(ts),
                            LineItem::String("i".to_string()),
                            LineItem::String(chars.to_string()),
                        ];
                        let line = serde_json::to_string(&data).unwrap() + "\n";
                        // .ok(): if the writer thread has already exited (session
                        // ended) we simply discard the event.
                        stdin_event_tx.send(Some(line)).ok();
                        pending_bytes.drain(..valid_up_to);
                    } else {
                        trace!("stdin: buffering incomplete UTF-8 sequence");
                    }
                }

                stdin_tx.send((buf.to_vec(), n)).unwrap();
            }
        });

        // The stdout thread owns the remaining (non-cloned) event_tx so that the
        // writer thread's channel is closed when this thread exits.
        let stdout_event_tx = event_tx;
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
                            // Signal the writer thread that recording is done.
                            stdout_event_tx.send(None).ok();
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
                            let chars =
                                std::str::from_utf8(&pending_bytes[..valid_up_to]).unwrap();

                            // https://github.com/asciinema/asciinema/blob/5a385765f050e04523c9d74fbf98d5afaa2deff0/asciinema/asciicast/v2.py#L119
                            let data = vec![
                                LineItem::F64(ts),
                                LineItem::String("o".to_string()),
                                LineItem::String(chars.to_string()),
                            ];
                            let line = serde_json::to_string(&data).unwrap() + "\n";
                            stdout_event_tx.send(Some(line)).ok();

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

        // Dedicated writer thread: drains the event channel and writes lines to the
        // cast file in arrival order, eliminating races between the stdin/stdout threads.
        let output_writer = self.output_writer.clone();
        thread::spawn(move || {
            loop {
                match event_rx.recv() {
                    Ok(Some(line)) => {
                        output_writer
                            .lock()
                            .expect("failed to acquire output writer lock")
                            .write_all(line.as_bytes())
                            .expect("failed to write event to cast file");
                    }
                    // None = done signal from stdout thread; Err = channel closed.
                    Ok(None) | Err(_) => break,
                }
            }
        });

        #[cfg(windows)]
        {
            self.terminal.attach_stdin(stdin_rx);
            self.terminal.attach_stdout(stdout_tx);
            self.terminal.run(&self.command).unwrap();
        }
        #[cfg(not(windows))]
        {
            drop(stdin_rx);
            drop(stdout_tx);
            eprintln!("error: recording is only supported on Windows");
            std::process::exit(1);
        }
    }
}
