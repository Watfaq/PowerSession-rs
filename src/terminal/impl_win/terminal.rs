use crate::terminal::Terminal;

use std::option::Option;

use std::ptr::null_mut;
use std::sync::mpsc::{Receiver, Sender};

use super::process::start_process;

use log::trace;
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
    SetConsoleMode, CONSOLE_MODE, CONSOLE_SCREEN_BUFFER_INFO, COORD, DISABLE_NEWLINE_AUTO_RETURN,
    ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_LVB_GRID_WORLDWIDE, ENABLE_PROCESSED_INPUT,
    ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    ENABLE_WRAP_AT_EOL_OUTPUT, HPCON,
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
        unsafe {
            if let Ok((x, y)) = WindowsTerminal::get_console_size() {
                console_size.X = x;
                console_size.Y = y;
            }

            WindowsTerminal::set_raw_mode()?;

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

    unsafe fn set_raw_mode() -> Result<()> {
        WindowsTerminal::set_raw_mode_on_stdin()?;
        WindowsTerminal::set_raw_mode_on_stdout()
    }

    unsafe fn set_raw_mode_on_stdin() -> Result<()> {
        let mut console_mode = CONSOLE_MODE::default();
        let handle = CreateFileW(
            "CONIN$",
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            HANDLE::default(),
        )
        .unwrap();

        GetConsoleMode(handle, &mut console_mode).expect("get console mode");

        console_mode &= !ENABLE_ECHO_INPUT;
        console_mode &= !ENABLE_LINE_INPUT;
        console_mode &= !ENABLE_PROCESSED_INPUT;

        console_mode |= ENABLE_VIRTUAL_TERMINAL_INPUT;

        SetConsoleMode(handle, console_mode).expect("set console mode");

        Ok(())
    }

    unsafe fn set_raw_mode_on_stdout() -> Result<()> {
        let mut console_mode = CONSOLE_MODE::default();
        let handle = CreateFileW(
            "CONOUT$",
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            HANDLE::default(),
        )
        .unwrap();

        GetConsoleMode(handle, &mut console_mode).expect("get console mode");

        console_mode |= ENABLE_PROCESSED_OUTPUT;
        console_mode |= ENABLE_VIRTUAL_TERMINAL_PROCESSING;
        //console_mode |= DISABLE_NEWLINE_AUTO_RETURN;
        //console_mode &= !ENABLE_WRAP_AT_EOL_OUTPUT;

        SetConsoleMode(handle, console_mode).expect("set console mode");

        Ok(())
    }

    unsafe fn get_console_size() -> Result<(i16, i16)> {
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

        let mut csbi = CONSOLE_SCREEN_BUFFER_INFO::default();

        if GetConsoleScreenBufferInfo(h_console, &mut csbi).as_bool() {
            Ok((
                csbi.srWindow.Right - csbi.srWindow.Left + 1,
                csbi.srWindow.Bottom - csbi.srWindow.Top + 1,
            ))
        } else {
            Ok((140, 80))
        }
    }
}

impl Terminal for WindowsTerminal {
    fn run(&mut self, command: &str) -> crate::terminal::Result<u32> {
        let process = start_process(command, &self.cwd, &mut self.handle);
        unsafe {
            WaitForSingleObject(process.process_info.hProcess, INFINITE);
            let mut exit_code: u32 = 0;

            GetExitCodeProcess(process.process_info.hProcess, &mut exit_code);

            trace!("process {} exited, exit code: {}", command, exit_code);

            return Ok(exit_code);
        };
    }

    fn attach_stdin(&self, rx: Receiver<(Arc<[u8]>, usize)>) {
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
    fn attach_stdout(&self, tx: Sender<(Arc<[u8]>, usize)>) {
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
                    trace!("read stdout error: {}", Error::from_win32().message());
                    tx.send((Arc::from(buf), 0)).unwrap();
                    break;
                }
            }

            tx.send((Arc::from(buf), n_read as _)).unwrap();
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
            if !self.stdin.is_invalid() {
                trace!("closing PseudoConsole stdin");
                CloseHandle(self.stdin);
            }
            if !self.stdout.is_invalid() {
                trace!("closing PseudoConsole stdout");
                CloseHandle(self.stdout);
            }
        }
    }
}
