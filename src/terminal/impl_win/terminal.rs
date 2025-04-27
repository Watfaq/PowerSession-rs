use crate::terminal::Terminal;

use std::option::Option;

use std::sync::mpsc::{Receiver, Sender};

use super::process::start_process;

use log::trace;
use windows::core::{Error, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE, INVALID_HANDLE_VALUE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ,
    FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, GetConsoleMode, GetConsoleScreenBufferInfo, SetConsoleMode,
    CONSOLE_MODE, CONSOLE_SCREEN_BUFFER_INFO, COORD, ENABLE_ECHO_INPUT,
    ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, ENABLE_PROCESSED_OUTPUT,
    ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, HPCON,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, WaitForSingleObject, INFINITE,
};

pub struct WindowsTerminal {
    handle: HPCON,
    stdin: isize,
    stdout: isize,
    cwd: String,

    pub width: i16,
    pub height: i16,
}

impl WindowsTerminal {
    pub fn new(cwd: Option<String>) -> Self {
        let mut handle = HPCON::default();
        let mut stdin = INVALID_HANDLE_VALUE;
        let mut stdout = INVALID_HANDLE_VALUE;
        let (width, height) =
            WindowsTerminal::create_pseudo_console_and_pipes(&mut handle, &mut stdin, &mut stdout)
                .expect("failed to create pseudo console");

        WindowsTerminal {
            handle,
            stdin: stdin.0 as isize,
            stdout: stdout.0 as isize,
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
    ) -> Result<(i16, i16)> {
        let mut h_pipe_pty_in = INVALID_HANDLE_VALUE;
        let mut h_pipe_pty_out = INVALID_HANDLE_VALUE;

        unsafe {
            CreatePipe(&mut h_pipe_pty_in, stdin, None, 0)?;
            CreatePipe(stdout, &mut h_pipe_pty_out, None, 0)?;
        }

        let mut console_size = COORD::default();
        unsafe {
            if let Ok((x, y)) = WindowsTerminal::get_console_size() {
                console_size.X = x;
                console_size.Y = y;
            }

            WindowsTerminal::set_raw_mode()?;

            *handle = CreatePseudoConsole(console_size, h_pipe_pty_in, h_pipe_pty_out, 0)?;

            let _ = CloseHandle(h_pipe_pty_in);
            let _ = CloseHandle(h_pipe_pty_out);

            Ok((console_size.X, console_size.Y))
        }
    }

    fn clone_handle(handle: HANDLE) -> Result<HANDLE> {
        let mut rv = HANDLE::default();
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                handle,
                GetCurrentProcess(),
                &mut rv,
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )?;
        }
        Ok(rv)
    }

    unsafe fn set_raw_mode() -> Result<()> {
        unsafe {
            WindowsTerminal::set_raw_mode_on_stdin()?;
            WindowsTerminal::set_raw_mode_on_stdout()
        }
    }

    unsafe fn set_raw_mode_on_stdin() -> Result<()> {
        unsafe {
            let mut console_mode = CONSOLE_MODE::default();
            let handle = CreateFileW(
                PCWSTR(HSTRING::from("CONIN$").as_ptr()),
                (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )?;

            GetConsoleMode(handle, &mut console_mode).expect("get console mode");

            console_mode &= !ENABLE_ECHO_INPUT;
            console_mode &= !ENABLE_LINE_INPUT;
            console_mode &= !ENABLE_PROCESSED_INPUT;

            console_mode |= ENABLE_VIRTUAL_TERMINAL_INPUT;

            SetConsoleMode(handle, console_mode).expect("set console mode");

            Ok(())
        }
    }

    unsafe fn set_raw_mode_on_stdout() -> Result<()> {
        unsafe {
            let mut console_mode = CONSOLE_MODE::default();
            let handle = CreateFileW(
                PCWSTR(HSTRING::from("CONOUT$").as_ptr()),
                (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
            .expect("create console mode");

            GetConsoleMode(handle, &mut console_mode).expect("get console mode");

            console_mode |= ENABLE_PROCESSED_OUTPUT;
            console_mode |= ENABLE_VIRTUAL_TERMINAL_PROCESSING;

            SetConsoleMode(handle, console_mode).expect("set console mode");

            Ok(())
        }
    }

    unsafe fn get_console_size() -> Result<(i16, i16)> {
        unsafe {
            let h_console = CreateFileW(
                PCWSTR(HSTRING::from("CONOUT$").as_ptr()),
                (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
            .expect("create console mode");

            if h_console == INVALID_HANDLE_VALUE {
                return Err(Error::from_win32());
            }

            let mut csbi = CONSOLE_SCREEN_BUFFER_INFO::default();

            if GetConsoleScreenBufferInfo(h_console, &mut csbi).is_ok() {
                Ok((
                    csbi.srWindow.Right - csbi.srWindow.Left + 1,
                    csbi.srWindow.Bottom - csbi.srWindow.Top + 1,
                ))
            } else {
                Ok((140, 80))
            }
        }
    }
}

impl Terminal for WindowsTerminal {
    fn run(&mut self, command: &str) -> crate::terminal::Result<u32> {
        let process = start_process(command, &self.cwd, &mut self.handle);
        unsafe {
            WaitForSingleObject(process.process_info.hProcess, INFINITE);
            let mut exit_code: u32 = 0;

            GetExitCodeProcess(process.process_info.hProcess, &mut exit_code)
                .expect("get exit code");

            trace!("process {} exited, exit code: {}", command, exit_code);

            Ok(exit_code)
        }
    }

    fn attach_stdin(&self, rx: Receiver<(Vec<u8>, usize)>) {
        let h = HANDLE(self.stdin as _);
        if h.is_invalid() {
            panic!("input handle invalid");
        }
        let stdin = WindowsTerminal::clone_handle(h).unwrap().0 as isize;

        std::thread::spawn(move || {
            loop {
                let (buf, n) = rx.recv().unwrap();

                unsafe {
                    if !WriteFile(HANDLE(stdin as _), Some(&buf[..n]), None, None).is_ok() {
                        break;
                    }
                }
            }
        });
    }
    fn attach_stdout(&self, tx: Sender<(Vec<u8>, usize)>) {
        let h = HANDLE(self.stdout as _);
        if h.is_invalid() {
            panic!("stdout handle invalid");
        }

        let stdout = WindowsTerminal::clone_handle(h).unwrap().0 as isize;

        std::thread::spawn(move || {
            loop {
                let mut buf = [0; 1024];
                let mut n_read = 0;
                unsafe {
                    if !ReadFile(HANDLE(stdout as _), Some(&mut buf), Some(&mut n_read), None)
                        .is_ok()
                    {
                        // The stdout is closed. send 0 to indicate read end.
                        trace!("read stdout error: {}", Error::from_win32().message());
                        tx.send((buf.to_vec(), 0)).unwrap();
                        break;
                    }
                }

                tx.send((buf.to_vec(), n_read as _)).unwrap();
            }
        });
    }
}

impl Drop for WindowsTerminal {
    fn drop(&mut self) {
        trace!("dropping WindowsTerminal");

        unsafe {
            if !self.handle.is_invalid() {
                trace!("closing PseudoConsole handle");
                ClosePseudoConsole(self.handle);
            }
            if !HANDLE(self.stdin as _).is_invalid() {
                trace!("closing PseudoConsole stdin");
                let _ = CloseHandle(HANDLE(self.stdin as _));
            }
            if !HANDLE(self.stdout as _).is_invalid() {
                trace!("closing PseudoConsole stdout");
                let _ = CloseHandle(HANDLE(self.stdout as _));
            }
        }
    }
}
