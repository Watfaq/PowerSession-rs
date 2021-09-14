use std::ptr::null_mut;

use super::bindings::Windows::Win32::Foundation::*;
use super::bindings::Windows::Win32::System::Console::*;
use super::bindings::Windows::Win32::System::Threading::*;

extern crate windows as w;
use w::HRESULT;

static PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x0002_0016;

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

            // Cleanup attribute list
            DeleteProcThreadAttributeList(self.startup_info.lpAttributeList);
        }
    }
}

pub fn start_process(command: &str, working_dir: &str, h_pc: &mut HPCON) -> Process {
    let mut startup_info = configure_process_thread(h_pc).expect("couldn't setup startup_info");
    let process_info =
        run_process(&mut startup_info, command, working_dir).expect("couldn't start process");
    Process {
        startup_info,
        process_info,
    }
}

fn configure_process_thread(h_pc: &mut HPCON) -> w::Result<STARTUPINFOEXW> {
    let mut start_info = STARTUPINFOEXW::default();
    start_info.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;

    let mut lp_size: usize = 0;
    let mut success: BOOL;
    unsafe {
        success = InitializeProcThreadAttributeList(
            LPPROC_THREAD_ATTRIBUTE_LIST::NULL,
            1,
            0,
            &mut lp_size,
        );
        // Note: This initial call will return an error by design. This is expected behavior.
        if success.as_bool() || lp_size == 0 {
            let err = HRESULT::from_thread();
            return Err(err.into());
        }
    }

    let mut lp_attribute_list: Box<[u8]> = vec![0; lp_size].into_boxed_slice();
    start_info.lpAttributeList =
        LPPROC_THREAD_ATTRIBUTE_LIST(lp_attribute_list.as_mut_ptr().cast::<_>());

    success = unsafe {
        InitializeProcThreadAttributeList(start_info.lpAttributeList, 1, 0, &mut lp_size)
    };
    if !success.as_bool() {
        let err = HRESULT::from_thread();
        return Err(err.into());
    }

    success = unsafe {
        UpdateProcThreadAttribute(
            start_info.lpAttributeList,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
            h_pc.0 as _,
            std::mem::size_of::<HPCON>(),
            null_mut(),
            null_mut(),
        )
    };

    if !success.as_bool() {
        let err = HRESULT::from_thread();
        return Err(err.into());
    }

    Ok(start_info)
}

fn run_process(
    startup_info: &mut STARTUPINFOEXW,
    command: &str,
    working_dir: &str,
) -> w::Result<PROCESS_INFORMATION> {
    let mut p_info = PROCESS_INFORMATION::default();

    let success = unsafe {
        CreateProcessW(
            PWSTR::NULL,
            command,
            null_mut(),
            null_mut(),
            false,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            null_mut(),
            working_dir,
            &mut startup_info.StartupInfo,
            &mut p_info,
        )
    };

    if !success.as_bool() {
        let err = HRESULT::from_thread();
        return Err(err.into());
    }

    Ok(p_info)
}
