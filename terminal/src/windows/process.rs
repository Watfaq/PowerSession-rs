use std::mem;
use std::ptr::null_mut;

use super::bindings::Windows::Win32::Debug::GetLastError;
use super::bindings::Windows::Win32::SystemServices::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
    UpdateProcThreadAttribute, BOOL, HPCON, INVALID_HANDLE_VALUE, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROCESS_INFORMATION, SECURITY_ATTRIBUTES, STARTUPINFOEXW, STARTUPINFOW,
};
use super::bindings::Windows::Win32::WindowsProgramming::{CloseHandle, PROCESS_CREATION_FLAGS};

static PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x00020016;

pub struct Process {
    pub startup_info: STARTUPINFOEXW,
    pub process_info: PROCESS_INFORMATION,
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            // TODO: check null???
            DeleteProcThreadAttributeList(self.startup_info.lpAttributeList);
            if self.process_info.hProcess != INVALID_HANDLE_VALUE {
                CloseHandle(self.process_info.hProcess);
            }
            if self.process_info.hThread != INVALID_HANDLE_VALUE {
                CloseHandle(self.process_info.hThread);
            }
        }
    }
}

pub fn start_process(command: &str, working_dir: &str, hPC: &mut HPCON) -> Process {
    let mut startup_info = configure_process_thread(hPC, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE);
    let process_info = run_process(&mut startup_info.StartupInfo, command, working_dir);
    Process {
        startup_info: startup_info,
        process_info: process_info,
    }
}

fn configure_process_thread(hPC: &mut HPCON, attributes: usize) -> STARTUPINFOEXW {
    let mut lpSize: usize = 0;
    let mut success: BOOL;
    unsafe {
        success = InitializeProcThreadAttributeList(
            LPPROC_THREAD_ATTRIBUTE_LIST(null_mut()),
            1,
            0,
            &mut lpSize,
        );
        if success.as_bool() || lpSize == 0 {
            panic!("Can't calculate the number of bytes for the attribute list");
            // TODO: get last_error here;
        }
    }

    let mut lpAttributeList: Box<[u8]> = vec![0; lpSize].into_boxed_slice();
    let start_info = STARTUPINFOEXW {
        StartupInfo: STARTUPINFOW {
            cb: mem::size_of::<STARTUPINFOEXW>() as u32,
            ..Default::default()
        },
        lpAttributeList: LPPROC_THREAD_ATTRIBUTE_LIST(lpAttributeList.as_mut_ptr().cast::<_>()),
    };

    success =
        unsafe { InitializeProcThreadAttributeList(start_info.lpAttributeList, 1, 0, &mut lpSize) };
    if !success.as_bool() {
        panic!("Cant setup attribute list");
    }

    success = unsafe {
        UpdateProcThreadAttribute(
            start_info.lpAttributeList,
            0,
            attributes,
            (hPC as *mut HPCON).cast::<std::ffi::c_void>(),
            std::mem::size_of::<HPCON>(),
            null_mut(),
            null_mut(),
        )
    };

    if !success.as_bool() {
        panic!("Cant set pseudoconsole thread attribute");
    }

    return start_info;
}

fn run_process(
    startup_info: &mut STARTUPINFOW,
    command: &str,
    working_dir: &str,
) -> PROCESS_INFORMATION {
    let mut p_info: PROCESS_INFORMATION = unsafe {
        {
            mem::zeroed()
        }
    };
    let securityAttributeSize = mem::size_of::<SECURITY_ATTRIBUTES>() as u32;
    let mut p_sec = SECURITY_ATTRIBUTES::default();
    p_sec.nLength = securityAttributeSize;
    let mut t_sec = SECURITY_ATTRIBUTES::default();
    t_sec.nLength = securityAttributeSize;

    let success = unsafe {
        CreateProcessW(
            "",
            command,
            &mut p_sec,
            &mut t_sec,
            false,
            PROCESS_CREATION_FLAGS::EXTENDED_STARTUPINFO_PRESENT,
            null_mut(),
            working_dir,
            startup_info,
            &mut p_info,
        )
    };

    if !success.as_bool() {
        let err = unsafe { GetLastError() };
        panic!("Cant create process: {:?}", err);
    }
    return p_info;
}
