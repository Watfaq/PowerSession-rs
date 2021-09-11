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

pub struct WindowsTerminal {
    handle: HPCON,
    stdin: HANDLE,
    stdout: HANDLE,
    cwd: String,

    pub width: i16,
    pub height: i16,
}

impl WindowsTerminal {
    pub fn new(cwd: String) -> Self {
        let mut handle = HPCON::NULL;
        let mut stdin = INVALID_HANDLE_VALUE;
        let mut stdout = INVALID_HANDLE_VALUE;
        let (width, height) =
            WindowsTerminal::create_pseudo_console_and_pipes(&mut handle, &mut stdin, &mut stdout);
        WindowsTerminal {
            handle,
            stdin,
            stdout,
            cwd,
            width,
            height,
        }
    }

    fn create_pseudo_console_and_pipes(
        handle: &mut HPCON,
        stdin: &mut HANDLE,  // the stdin to write input to PTY
        stdout: &mut HANDLE, // the stdout to read output from PTY
    ) -> (i16, i16) {
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
            if h_console == INVALID_HANDLE_VALUE {
                let err = HRESULT::from_thread();
                panic!("Cannot get stdout: {}", err.message());
            }
            if GetConsoleScreenBufferInfo(h_console, &mut csbi).as_bool() {
                console_size.X = csbi.srWindow.Right - csbi.srWindow.Left + 1;
                console_size.Y = csbi.srWindow.Bottom - csbi.srWindow.Top + 1;
            } else {
                console_size.X = 140;
                console_size.Y = 80;
            }

            *handle = CreatePseudoConsole(console_size, h_pipe_pty_in, h_pipe_pty_out, 0)
                .expect("Cant create PseudoConsole");
            (console_size.X, console_size.Y)
        }
    }
}

impl Terminal for WindowsTerminal {
    fn run(&mut self, command: &str) -> u32 {
        let process = start_process(command, &self.cwd, &mut self.handle);
        unsafe {
            WaitForSingleObject(process.process_info.hProcess, INFINITE);

            if !CloseHandle(self.stdin).as_bool() {
                panic!(HRESULT::from_thread());
            }
            if !CloseHandle(self.stdout).as_bool() {
                panic!(HRESULT::from_thread());
            }
            ClosePseudoConsole(self.handle);
        }

        let mut exit_code: u32 = 0;
        unsafe {
            GetExitCodeProcess(process.process_info.hProcess, &mut exit_code);
        }
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
                    println!("cannot read stdout");
                    break;
                }
            }
            tx.send(buf[0]).unwrap();
        });
    }
}
