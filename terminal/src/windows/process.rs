use std::ffi::OsStr;
use std::mem;
use std::os::windows::prelude::*;
use std::ptr::null_mut;

use super::bindings::Windows::Win32::System::Diagnostics::Debug::*;
use super::bindings::Windows::Win32::System::SystemServices::*;
use super::bindings::Windows::Win32::System::Threading::*;
use super::bindings::Windows::Win32::System::WindowsProgramming::*;
extern crate windows as w;
use w::HRESULT;

static PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x00020016;

pub struct Process {
    pub startup_info: STARTUPINFOEXW,
    pub process_info: PROCESS_INFORMATION,
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            if self.process_info.hProcess != INVALID_HANDLE_VALUE {
                CloseHandle(self.process_info.hProcess);
            }
            if self.process_info.hThread != INVALID_HANDLE_VALUE {
                CloseHandle(self.process_info.hThread);
            }
        }
    }
}

pub fn start_process(command: &str, working_dir: &str, h_pc: &mut HPCON) -> Process {
    let mut startup_info = configure_process_thread(h_pc, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE);
    let process_info = run_process(&mut startup_info, command, working_dir);
    Process {
        startup_info,
        process_info,
    }
}

fn configure_process_thread(h_pc: &mut HPCON, attributes: usize) -> STARTUPINFOEXW {
    let mut lp_size: usize = 0;
    let mut success: BOOL;
    unsafe {
        success = InitializeProcThreadAttributeList(
            LPPROC_THREAD_ATTRIBUTE_LIST(null_mut()),
            1,
            0,
            &mut lp_size,
        );
        if success.as_bool() || lp_size == 0 {
            let err = GetLastError();
            let rv = HRESULT(err.0);
            panic!(
                "Can't calculate the number of bytes for the attribute list, {}",
                rv.message()
            );
        }
    }

    let mut lp_attribute_list: Box<[u8]> = vec![0; lp_size].into_boxed_slice();
    let start_info = STARTUPINFOEXW {
        StartupInfo: STARTUPINFOW {
            cb: mem::size_of::<STARTUPINFOEXW>() as u32,
            ..Default::default()
        },
        lpAttributeList: LPPROC_THREAD_ATTRIBUTE_LIST(lp_attribute_list.as_mut_ptr().cast::<_>()),
    };

    success = unsafe {
        InitializeProcThreadAttributeList(start_info.lpAttributeList, 1, 0, &mut lp_size)
    };
    if !success.as_bool() {
        let err = unsafe { GetLastError() };
        let rv = HRESULT(err.0);
        panic!("Can't setup attribute list, {}", rv.message());
    }

    success = unsafe {
        UpdateProcThreadAttribute(
            start_info.lpAttributeList,
            0,
            attributes,
            (h_pc as *mut HPCON).cast::<std::ffi::c_void>(),
            std::mem::size_of::<HPCON>(),
            null_mut(),
            null_mut(),
        )
    };

    if !success.as_bool() {
        let err = unsafe { GetLastError() };
        let rv = HRESULT(err.0);
        panic!("Can't setup process attribute, {}", rv.message());
    }

    return start_info;
}

fn run_process(
    startup_info: &mut STARTUPINFOEXW,
    command: &str,
    working_dir: &str,
) -> PROCESS_INFORMATION {
    let mut p_info: PROCESS_INFORMATION = unsafe {
        {
            mem::zeroed()
        }
    };
    let security_attribute_size = mem::size_of::<SECURITY_ATTRIBUTES>() as u32;
    let mut p_sec = SECURITY_ATTRIBUTES::default();
    p_sec.nLength = security_attribute_size;
    let mut t_sec = SECURITY_ATTRIBUTES::default();
    t_sec.nLength = security_attribute_size;

    let success = unsafe {
        CreateProcessW(
            PWSTR(null_mut()),
            PWSTR(
                OsStr::new(command)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect::<Vec<_>>()
                    .as_mut_ptr(),
            ),
            &mut p_sec,
            &mut t_sec,
            false,
            EXTENDED_STARTUPINFO_PRESENT,
            null_mut(),
            PWSTR(
                OsStr::new(working_dir)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect::<Vec<_>>()
                    .as_mut_ptr(),
            ),
            &mut startup_info.StartupInfo,
            &mut p_info,
        )
    };

    if !success.as_bool() {
        let err = unsafe { GetLastError() };
        let result = HRESULT(err.0);
        panic!("Cant create process: {:?}", result.message());
    }
    return p_info;
}
