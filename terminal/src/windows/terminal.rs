use std::ptr::null_mut;
use std::sync::mpsc::{Receiver, Sender};

use super::bindings::Windows::Win32::Storage::FileSystem::*;
use super::bindings::Windows::Win32::System::Console::*;
use super::bindings::Windows::Win32::System::Diagnostics::Debug::GetLastError;
use super::bindings::Windows::Win32::System::SystemServices::*;
use super::bindings::Windows::Win32::System::WindowsProgramming::*;
use super::bindings::Windows::Win32::System::Threading::*;
extern crate windows as w;
use w::HRESULT;

use super::process::start_process;

use crate::Terminal;

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
        let mut handle = HPCON::NULL;
        let mut stdin = INVALID_HANDLE_VALUE;
        let mut stdout = INVALID_HANDLE_VALUE;
        WindowsTerminal::create_pseudo_console_and_pipes(&mut handle, &mut stdin, &mut stdout);
        WindowsTerminal {
            handle,
            stdin,
            stdout,
            cwd,
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
            if !CreatePipe(&mut h_pipe_pty_in, stdin, null_mut(), 0).as_bool() {
                panic!("cannot create pipe");
            }
            if !CreatePipe(stdout, &mut h_pipe_pty_out, null_mut(), 0).as_bool() {
                panic!("cannot create pipe");
            }
        }

        let mut console_size = COORD::default();
        let mut csbi = CONSOLE_SCREEN_BUFFER_INFO::default();
        unsafe {
            let h_console = GetStdHandle(STD_OUTPUT_HANDLE);
            if GetConsoleScreenBufferInfo(h_console, &mut csbi).as_bool() {
                console_size.X = csbi.srWindow.Right - csbi.srWindow.Left + 1;
                console_size.Y = csbi.srWindow.Bottom - csbi.srWindow.Top + 1;
            } else {
                let err = GetLastError();
                if err.0 == 5 || err.0 == 6 {
                    // let's assume we are in debug
                    // https://github.com/microsoft/vscode-cpptools/issues/5449
                    console_size.X = 140;
                    console_size.Y = 80;
                } else {
                    let rv = HRESULT(err.0);
                    panic!("Cannot get screen buffer info, {}", rv.message());
                }
            }

            let result =
                CreatePseudoConsole(console_size, h_pipe_pty_in, h_pipe_pty_out, 0, handle);
            if result.is_err() {
                panic!("Cant create PseudoConsole: {:?}", result.message());
            }
        }
    }
}

impl Terminal for WindowsTerminal {
    fn run(&mut self, command: &str) -> u32 {
        let process = start_process(command, &self.cwd, &mut self.handle);
        unsafe { WaitForSingleObject(process.process_info.hProcess, INFINITE); }
        let mut exit_code: u32 = 0;
        unsafe { GetExitCodeProcess(process.process_info.hProcess, &mut exit_code); }
        return exit_code;
    }

    fn attach_stdin(&self, rx: Receiver<u8>) {
        let stdin = self.stdin;
        std::thread::spawn(move || loop {
            let rv = rx.recv();
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
                    println!("cannot receive on rx: {}", err);
                    break;
                }
            }
        });
    }
    fn attach_stdout(&self, tx: Sender<u8>) {
        let stdout = self.stdout;
        std::thread::spawn(move || loop {
            let mut buf = [0; 1];
            let mut n_read: u32 = 0;
            unsafe {
                let success = ReadFile(stdout, buf.as_mut_ptr() as _, 1, &mut n_read, null_mut());
                println!("{}, {}", success.as_bool(), n_read);
                if !success.as_bool() || n_read == 0 {
                    println!("{}", "cannot read stdout");
                    break;
                }
            }
            let rv = tx.send(buf[0]);
            match rv {
                Ok(_) => (),
                Err(_) => break,
            }
        });
    }
}
