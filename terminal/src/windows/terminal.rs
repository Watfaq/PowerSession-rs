use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::option::Option;
use std::os::windows::io::FromRawHandle;
use std::ptr::null_mut;
use std::sync::mpsc::{Receiver, Sender};

use super::bindings::Windows::Win32::Foundation::*;
use super::bindings::Windows::Win32::Storage::FileSystem::*;
use super::bindings::Windows::Win32::System::Console::*;
use super::bindings::Windows::Win32::System::Pipes::*;
use super::bindings::Windows::Win32::System::Threading::*;
use super::bindings::Windows::Win32::System::WindowsProgramming::*;

extern crate windows as w;
use w::HRESULT;

use super::process::start_process;

use crate::Terminal;
use std::sync::Arc;

pub struct WindowsTerminal {
    handle: HPCON,
    stdin: HANDLE,
    stdout: HANDLE,
    cwd: String,

    pub width: i16,
    pub height: i16,
}

impl WindowsTerminal {
    pub fn new(cwd: Option<String>) -> Self {
        let mut handle = HPCON::NULL;
        let mut stdin = INVALID_HANDLE_VALUE;
        let mut stdout = INVALID_HANDLE_VALUE;
        let (width, height) =
            WindowsTerminal::create_pseudo_console_and_pipes(&mut handle, &mut stdin, &mut stdout)
                .expect("failed to create pseduo console");

        WindowsTerminal {
            handle,
            stdin,
            stdout,
            cwd: cwd.unwrap_or_else(|| {
                std::env::current_dir()
                    .expect("failed to get cwd")
                    .into_os_string()
                    .into_string()
                    .unwrap()
            }),
            width,
            height,
        }
    }

    fn create_pseudo_console_and_pipes(
        handle: &mut HPCON,
        stdin: &mut HANDLE,  // the stdin to write input to PTY
        stdout: &mut HANDLE, // the stdout to read output from PTY
    ) -> w::Result<(i16, i16)> {
        let mut h_pipe_pty_in = INVALID_HANDLE_VALUE;
        let mut h_pipe_pty_out = INVALID_HANDLE_VALUE;

        unsafe {
            CreatePipe(&mut h_pipe_pty_in, stdin, null_mut(), 0).ok()?;
            CreatePipe(stdout, &mut h_pipe_pty_out, null_mut(), 0).ok()?;
        }

        let mut console_size = COORD::default();
        let mut csbi = CONSOLE_SCREEN_BUFFER_INFO::default();
        unsafe {
            let h_console = CreateFileW(
                "CONOUT$",
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                HANDLE::NULL,
            );

            if h_console == INVALID_HANDLE_VALUE {
                return Err(HRESULT::from_thread().into());
            }

            if GetConsoleScreenBufferInfo(h_console, &mut csbi).as_bool() {
                console_size.X = csbi.srWindow.Right - csbi.srWindow.Left + 1;
                console_size.Y = csbi.srWindow.Bottom - csbi.srWindow.Top + 1;
            } else {
                console_size.X = 140;
                console_size.Y = 80;
            }

            let mut console_mode = CONSOLE_MODE::default();

            let not_raw_mode_mask = ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT;
            GetConsoleMode(h_console, &mut console_mode).ok()?;
            SetConsoleMode(
                h_console,
                console_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING & !not_raw_mode_mask,
            )
            .ok()?;

            *handle = CreatePseudoConsole(console_size, h_pipe_pty_in, h_pipe_pty_out, 0)
                .expect("Cant create PseudoConsole");

            CloseHandle(h_pipe_pty_in);
            CloseHandle(h_pipe_pty_out);

            Ok((console_size.X, console_size.Y))
        }
    }
}

impl Terminal for WindowsTerminal {
    fn run(&mut self, command: &str) -> w::Result<u32> {
        let process = start_process(command, &self.cwd, &mut self.handle);
        unsafe {
            WaitForSingleObject(process.process_info.hProcess, INFINITE);

            CloseHandle(self.stdin).ok()?;
            CloseHandle(self.stdout).ok()?;

            let mut exit_code: u32 = 0;

            GetExitCodeProcess(process.process_info.hProcess, &mut exit_code);

            return Ok(exit_code);
        };
    }

    fn attach_stdin(&self, rx: Receiver<(Arc<[u8; 1]>, usize)>) {
        let mut stdin = unsafe { File::from_raw_handle(self.stdin.0 as _) };
        std::thread::spawn(move || loop {
            let rv = rx.recv();
            match rv {
                Ok(b) => {
                    stdin.write_all(&b.0[..b.1]).expect("failed to write stdin");
                }
                Err(err) => {
                    println!("cannot receive on rx: {}", err);
                    break;
                }
            }
        });
    }
    fn attach_stdout(&self, tx: Sender<(Arc<[u8; 1024]>, usize)>) {
        let mut stdout = unsafe { File::from_raw_handle(self.stdout.0 as _) };

        std::thread::spawn(move || loop {
            let mut buf = [0; 1024];
            match stdout.read(&mut buf) {
                Ok(n) if n > 0 => {
                    tx.send((Arc::from(buf), n)).unwrap();
                }
                _ => break,
            }
        });
    }
}

impl Drop for WindowsTerminal {
    fn drop(&mut self) {
        unsafe {
            if self.handle != HPCON::NULL {
                ClosePseudoConsole(self.handle);
            }
            if self.stdin != INVALID_HANDLE_VALUE {
                CloseHandle(self.stdin);
            }
            if self.stdout != INVALID_HANDLE_VALUE {
                CloseHandle(self.stdout);
            }
        }
    }
}
