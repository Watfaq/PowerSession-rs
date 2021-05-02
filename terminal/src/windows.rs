use crate::Terminal;

mod bindings {
    windows::include_bindings!();
}

use std::collections::HashMap;
use std::ffi::c_void;
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::iter::once;
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::ptr::{null, null_mut};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use bindings::Windows::Win32::Debug::GetLastError;
use bindings::Windows::Win32::SystemServices::{
    ClosePseudoConsole, CreatePipe, CreateProcessW, CreatePseudoConsole, GetConsoleMode,
    GetConsoleScreenBufferInfo, InitializeProcThreadAttributeList, SetConsoleMode,
    CONSOLE_SCREEN_BUFFER_INFO, COORD, HANDLE, HPCON, INVALID_HANDLE_VALUE, PROCESS_INFORMATION,
    STARTUPINFOW, STARTUPINFOW_FLAGS,
};
use bindings::Windows::Win32::WindowsProgramming::{
    CloseHandle, GetStdHandle, PROCESS_CREATION_FLAGS, STD_HANDLE_TYPE,
};

use bindings::Windows::Win32::FileSystem::{ReadFile, WriteFile};

pub struct WindowsTerminal {
    handle: HPCON,
    stdin: HANDLE,
    stdout: HANDLE,
    cwd: String,
}

impl Drop for WindowsTerminal {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.stdin);
            CloseHandle(self.stdout);
            ClosePseudoConsole(self.handle);
        }
    }
}

impl WindowsTerminal {
    pub fn new(cwd: String) -> Self {
        let mut handle = HPCON::default();
        let mut stdin = INVALID_HANDLE_VALUE;
        let mut stdout = INVALID_HANDLE_VALUE;
        WindowsTerminal::create_pseudo_console_and_pipes(&mut handle, &mut stdin, &mut stdout);
        WindowsTerminal {
            handle: handle,
            stdin: stdin,
            stdout: stdout,
            cwd: cwd,
        }
    }

    fn create_pseudo_console_and_pipes(
        handle: &mut HPCON,
        stdin: &mut HANDLE,
        stdout: &mut HANDLE,
    ) {
        let mut h_pipe_pty_in = INVALID_HANDLE_VALUE;
        let mut h_pipe_pty_out = INVALID_HANDLE_VALUE;

        unsafe {
            CreatePipe(&mut h_pipe_pty_in, stdin, null_mut(), 0);
            CreatePipe(stdout, &mut h_pipe_pty_out, null_mut(), 0);
        }

        let mut console_size = COORD::default();
        let mut csbi = CONSOLE_SCREEN_BUFFER_INFO::default();
        unsafe {
            let h_console = GetStdHandle(STD_HANDLE_TYPE::STD_OUTPUT_HANDLE);
            if GetConsoleScreenBufferInfo(h_console, &mut csbi).as_bool() {
                console_size.X = csbi.srWindow.Right - csbi.srWindow.Left + 1;
                console_size.Y = csbi.srWindow.Bottom - csbi.srWindow.Top + 1;
            }

            // TODO: check HRESULT: https://github.com/microsoft/windows-rs/issues/765
            CreatePseudoConsole(console_size, h_pipe_pty_in, h_pipe_pty_out, 0, handle);
            if INVALID_HANDLE_VALUE != h_pipe_pty_out {
                CloseHandle(h_pipe_pty_out);
            }
            if INVALID_HANDLE_VALUE != h_pipe_pty_in {
                CloseHandle(h_pipe_pty_in);
            }
        }
    }
}

impl Terminal for WindowsTerminal {
    fn run(&self, command: &str) {
        unsafe {
            let mut pi_proc_info: PROCESS_INFORMATION = { mem::zeroed() };
            let mut si_start_info: STARTUPINFOW = { mem::zeroed() };
            si_start_info.cb = mem::size_of::<STARTUPINFOW>() as u32;

            let success = CreateProcessW(
                None,
                command,
                null_mut(),
                null_mut(),
                false,
                PROCESS_CREATION_FLAGS::EXTENDED_STARTUPINFO_PRESENT,
                null_mut(),
                self.cwd.as_str(),
                &mut si_start_info as *mut STARTUPINFOW,
                &mut pi_proc_info as *mut PROCESS_INFORMATION,
            );

            if !success.as_bool() {
                let err = GetLastError();
                panic!("Cant create process: {:?}", err);
            } else {
                CloseHandle(pi_proc_info.hProcess);
                CloseHandle(pi_proc_info.hThread);
            }
        }
    }
    fn attach_stdin(&self, rx: Receiver<u8>) {
        let stdin = self.stdin.clone();
        std::thread::spawn(move || loop {
            let rv = rx.recv_timeout(Duration::from_secs(1));
            match rv {
                Ok(b) => {
                    let mut buf = [b, 1];
                    unsafe {
                        WriteFile(
                            stdin,
                            buf.as_mut_ptr() as _,
                            1 as u32,
                            null_mut(),
                            null_mut(),
                        );
                    }
                }
                Err(err) => {
                    println!("{}", err);
                    break;
                }
            }
        });
    }
    fn attach_stdout(&self, tx: Sender<u8>) {
        let stdout = self.stdout.clone();
        std::thread::spawn(move || loop {
            let mut buf = [0; 1];
            let mut n_read: u32 = 0;
            unsafe {
                let success = ReadFile(stdout, buf.as_mut_ptr() as _, 1, &mut n_read, null_mut());
                if !success.as_bool() || n_read == 0 {
                    break;
                }
            }
            tx.send(buf[0]);
        });
    }
}
