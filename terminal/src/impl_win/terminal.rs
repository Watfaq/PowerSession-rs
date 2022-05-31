use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::option::Option;
use std::os::windows::io::FromRawHandle;
use std::ptr::null_mut;
use std::sync::mpsc::{Receiver, Sender};

use super::process::start_process;

use crate::Terminal;
use std::sync::Arc;
use windows::core::{Error, Result};
use windows::Win32::Foundation::{
    CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE, INVALID_HANDLE_VALUE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, GetConsoleMode, GetConsoleScreenBufferInfo,
    SetConsoleMode, CONSOLE_MODE, CONSOLE_SCREEN_BUFFER_INFO, COORD, ENABLE_ECHO_INPUT,
    ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, HPCON,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, WaitForSingleObject,
};
use windows::Win32::System::WindowsProgramming::INFINITE;

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
        let mut handle = HPCON::default();
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
    ) -> Result<(i16, i16)> {
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
                HANDLE::default(),
            )
            .unwrap();

            if h_console == INVALID_HANDLE_VALUE {
                return Err(Error::from_win32());
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
            if !GetConsoleMode(h_console, &mut console_mode).as_bool() {
                return Err(Error::from_win32());
            }
            if !SetConsoleMode(
                h_console,
                console_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING & !not_raw_mode_mask,
            )
            .as_bool()
            {
                return Err(Error::from_win32());
            }

            *handle = CreatePseudoConsole(console_size, h_pipe_pty_in, h_pipe_pty_out, 0)?;

            CloseHandle(h_pipe_pty_in);
            CloseHandle(h_pipe_pty_out);

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
            )
            .ok()?;
        }
        Ok(rv)
    }
}

impl Terminal for WindowsTerminal {
    fn run(&mut self, command: &str) -> crate::Result<u32> {
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
        if self.stdin.is_invalid() {
            panic!("input handle invalid");
        }
        let stdin = WindowsTerminal::clone_handle(self.stdin).unwrap();

        std::thread::spawn(move || loop {
            let (buf, n) = rx.recv().unwrap();

            unsafe {
                if !WriteFile(stdin, buf.as_ptr() as _, n as _, &mut 0, null_mut()).as_bool() {
                    break;
                }
            }
        });
    }
    fn attach_stdout(&self, tx: Sender<(Arc<[u8; 1024]>, usize)>) {
        if self.stdout.is_invalid() {
            panic!("stdout handle invalid");
        }

        let stdout = WindowsTerminal::clone_handle(self.stdout).unwrap();

        std::thread::spawn(move || loop {
            let mut buf = [0; 1024];
            let mut n_read = 0;
            unsafe {
                if !ReadFile(stdout, buf.as_mut_ptr() as _, 1024, &mut n_read, null_mut()).as_bool()
                {
                    // The stdout is closed. send 0 to indicate read end.
                    tx.send((Arc::from(buf), n_read as _)).unwrap();
                    break;
                }
            }

            tx.send((Arc::from(buf), n_read as _)).unwrap();
        });
    }
}

impl Drop for WindowsTerminal {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_invalid() {
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
