use crate::Terminal;
mod bindings {
    windows::include_bindings!();
}

use std::collections::HashMap;
use std::ffi::OsString;
use std::iter::once;
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};

use bindings::Windows::Win32::SystemServices::{
    ClosePseudoConsole, CreatePipe, CreateProcessW, CreatePseudoConsole, GetConsoleMode,
    GetConsoleScreenBufferInfo, SetConsoleMode, CONSOLE_SCREEN_BUFFER_INFO, COORD, HANDLE, HPCON,
    INVALID_HANDLE_VALUE, PROCESS_INFORMATION, PWSTR, STARTUPINFOW, STARTUPINFOW_FLAGS,
};
use bindings::Windows::Win32::WindowsProgramming::{
    CloseHandle, GetStdHandle, PROCESS_CREATION_FLAGS, STD_HANDLE_TYPE,
};

pub struct WindowsTerminal<'a> {
    handle: HPCON,
    stdin: HANDLE,
    stdout: HANDLE,
    cwd: &'a str,
}

impl<'a> Drop for WindowsTerminal<'a> {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.stdin);
            CloseHandle(self.stdout);
            ClosePseudoConsole(self.handle);
        }
    }
}

impl<'a> WindowsTerminal<'a> {
    pub fn new(cwd: &'a str) -> Box<dyn Terminal + 'a> {
        let mut handle = HPCON::default();
        let mut stdin = INVALID_HANDLE_VALUE;
        let mut stdout = INVALID_HANDLE_VALUE;
        WindowsTerminal::create_pseudo_console_and_pipes(&mut handle, &mut stdin, &mut stdout);
        Box::new(WindowsTerminal {
            handle: handle,
            stdin: stdin,
            stdout: stdout,
            cwd: cwd,
        })
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

    fn convert_to_pstr(s: &str) -> PWSTR {
        let ss = OsString::from(s);
        let mut s_wide: Vec<u16> = ss.as_os_str().encode_wide().chain(once(0)).collect();
        let mut c = PWSTR::default();
        c.0 = s_wide.as_mut_ptr();
        return c;
    }
}

impl<'a> Terminal for WindowsTerminal<'a> {
    fn run(&self, command: &str) {
        unsafe {
            let mut pi_proc_info: PROCESS_INFORMATION = { mem::zeroed() };
            let mut si_start_info: STARTUPINFOW = { mem::zeroed() };
            si_start_info.cb = mem::size_of::<STARTUPINFOW>() as u32;
            si_start_info.hStdError = self.stdout;
            si_start_info.hStdOutput = self.stdout;
            si_start_info.hStdInput = self.stdin;
            si_start_info.dwFlags |= STARTUPINFOW_FLAGS::STARTF_USESTDHANDLES;

            CreateProcessW(
                PWSTR::default(),
                WindowsTerminal::convert_to_pstr(command),
                null_mut(),
                null_mut(),
                false,
                PROCESS_CREATION_FLAGS::EXTENDED_STARTUPINFO_PRESENT,
                null_mut(),
                WindowsTerminal::convert_to_pstr(self.cwd),
                &mut si_start_info as *mut STARTUPINFOW,
                &mut pi_proc_info as *mut PROCESS_INFORMATION,
            );
        }
    }
    fn get_stdin(&self) -> std::fs::File {
        todo!()
    }
    fn get_stdout(&self) -> std::fs::File {
        todo!()
    }
}
