use std::env;

#[cfg(windows)]
use std::{
    io::ErrorKind,
    sync::{mpsc::channel, Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};

#[cfg(windows)]
use log::{error, trace};
#[cfg(windows)]
use tungstenite::stream::MaybeTlsStream;
#[cfg(windows)]
use tungstenite::Message;

#[cfg(windows)]
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::ReadFile,
    System::Console::{GetStdHandle, WriteConsoleW, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
};

#[cfg(windows)]
use crate::commands::types::LineItem;
#[cfg(windows)]
use crate::terminal::{Terminal, WindowsTerminal};

pub struct Stream {
    ws_url: String,
    stream_url: String,
    auth_header: String,
    command: String,
    #[cfg(windows)]
    terminal: WindowsTerminal,
}

impl Stream {
    pub fn new(
        ws_url: String,
        stream_url: String,
        auth_header: String,
        command: Option<String>,
    ) -> Self {
        Stream {
            ws_url,
            stream_url,
            auth_header,
            command: command
                .unwrap_or_else(|| env::var("SHELL").unwrap_or("powershell.exe".to_owned())),
            #[cfg(windows)]
            terminal: WindowsTerminal::new(None),
        }
    }

    pub fn execute(&mut self) {
        #[cfg(not(windows))]
        {
            eprintln!("error: streaming is only supported on Windows");
            std::process::exit(1);
        }
        #[cfg(windows)]
        {
            println!("Streaming. Watch at: {}", self.stream_url);
            println!("Exit the shell/command to stop streaming.");
            self.stream();
        }
    }

    #[cfg(windows)]
    fn stream(&mut self) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("check your machine time");
        let record_start_time = now.as_secs() as f64 + now.subsec_nanos() as f64 * 1e-9;

        // Build the WebSocket connection request, including the Authorization header so
        // both freshly-created streams and reconnects to existing streams authenticate.
        let request = tungstenite::http::Request::builder()
            .uri(&self.ws_url)
            .header("Authorization", &self.auth_header)
            .body(())
            .expect("failed to build WebSocket request");

        let (mut ws, _) =
            tungstenite::connect(request).expect("failed to connect to stream server");
        match ws.get_mut() {
            MaybeTlsStream::Plain(stream) => {
                stream
                    .set_nonblocking(true)
                    .unwrap_or_else(|e| trace!("failed to set non-blocking websocket: {}", e));
            }
            MaybeTlsStream::Rustls(stream) => {
                stream
                    .get_mut()
                    .set_nonblocking(true)
                    .unwrap_or_else(|e| trace!("failed to set non-blocking websocket: {}", e));
            }
            _ => {}
        }
        let ws = Arc::new(Mutex::new(ws));

        {
            // Send an asciicast-compatible reset event so the server knows the terminal size.
            let reset_data = format!("{}x{}", self.terminal.width, self.terminal.height);
            let reset_event = serde_json::to_string(&[
                LineItem::F64(0.0),
                LineItem::String("r".to_string()),
                LineItem::String(reset_data),
            ])
            .unwrap();
            let mut sock = ws.lock().expect("websocket mutex poisoned");
            sock.send(Message::Text(reset_event.into()))
                .expect("failed to send reset event");
        }

        // Keep a read loop alive to respond to Ping/Pong/Close frames from the server.
        let ws_reader = ws.clone();
        thread::spawn(move || loop {
            let msg = {
                let mut sock = ws_reader.lock().expect("websocket mutex poisoned");
                sock.read()
            };

            match msg {
                Ok(Message::Ping(payload)) => {
                    if let Ok(mut sock) = ws_reader.lock() {
                        let _ = sock.send(Message::Pong(payload));
                    }
                }
                Ok(Message::Close(frame)) => {
                    trace!("server closed stream: {:?}", frame);
                    break;
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut =>
                {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(tungstenite::Error::AlreadyClosed | tungstenite::Error::ConnectionClosed) => {
                    break;
                }
                Err(e) => {
                    error!("websocket read error: {}", e);
                    break;
                }
            }
        });

        let (stdin_tx, stdin_rx) = channel::<(Vec<u8>, usize)>();
        let (stdout_tx, stdout_rx) = channel::<(Vec<u8>, usize)>();

        // On Windows, use ReadFile directly to preserve ESC sequences (same as record.rs).
        let stdin_handle: isize = unsafe {
            GetStdHandle(STD_INPUT_HANDLE)
                .expect("failed to get Windows stdin handle (STD_INPUT_HANDLE)")
                .0 as isize
        };

        thread::spawn(move || loop {
            let mut buf = [0u8; 10];

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

            stdin_tx.send((buf.to_vec(), n)).unwrap();
        });

        // Stdout thread: read pty output, display it locally, and forward it to the WebSocket.
        let ws_writer = ws.clone();
        thread::spawn(move || {
            let stdout_handle: HANDLE = unsafe {
                GetStdHandle(STD_OUTPUT_HANDLE).expect("failed to get stdout handle")
            };

            let mut pending_bytes: Vec<u8> = Vec::new();

            loop {
                let rv = stdout_rx.recv();
                match rv {
                    Ok((buf, len)) => {
                        if len == 0 {
                            trace!("stdout received close indicator");
                            println!("\nStreaming session ended.");
                            if let Ok(mut sock) = ws_writer.lock() {
                                let _ = sock.close(None);
                            }
                            break;
                        }

                        let now = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .expect("check your machine time");
                        let ts = now.as_secs() as f64 + now.subsec_nanos() as f64 * 1e-9
                            - record_start_time;

                        pending_bytes.extend_from_slice(&buf[..len]);

                        let valid_up_to = match std::str::from_utf8(&pending_bytes) {
                            Ok(_) => pending_bytes.len(),
                            Err(e) => e.valid_up_to(),
                        };

                        if valid_up_to > 0 {
                            let chars =
                                std::str::from_utf8(&pending_bytes[..valid_up_to]).unwrap();

                            let data = vec![
                                LineItem::F64(ts),
                                LineItem::String("o".to_string()),
                                LineItem::String(chars.to_string()),
                            ];
                            let event = serde_json::to_string(&data).unwrap();

                            let send_result = ws_writer
                                .lock()
                                .expect("websocket mutex poisoned")
                                .send(Message::Text(event.into()));

                            if let Err(e) = send_result {
                                error!("failed to send WebSocket message: {}", e);
                                break;
                            }

                            // Echo output to the local console as well.
                            unsafe {
                                let utf16: Vec<u16> = chars.encode_utf16().collect();
                                WriteConsoleW(stdout_handle, &utf16, None, None)
                                    .expect("failed to write stdout");
                            }
                        }

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
