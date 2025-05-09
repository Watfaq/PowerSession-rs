use windows::core::{Error, Result, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows::Win32::System::Console::HPCON;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
    UpdateProcThreadAttribute, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
    STARTUPINFOEXW,
};

pub struct Process {
    pub startup_info: STARTUPINFOEXW,
    pub process_info: PROCESS_INFORMATION,
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            if self.process_info.hProcess != INVALID_HANDLE_VALUE {
                let _ = CloseHandle(self.process_info.hProcess);
            }
            if self.process_info.hThread != INVALID_HANDLE_VALUE {
                let _ = CloseHandle(self.process_info.hThread);
            }

            // Cleanup attribute list
            DeleteProcThreadAttributeList(self.startup_info.lpAttributeList);
        }
    }
}

pub fn start_process(command: &str, working_dir: &str, h_pc: &mut HPCON) -> Process {
    let mut startup_info =
        unsafe { configure_process_thread(h_pc) }.expect("couldn't setup startup_info");
    let process_info = unsafe { run_process(&mut startup_info, command, working_dir) }
        .expect("couldn't start process");
    Process {
        startup_info,
        process_info,
    }
}

unsafe fn configure_process_thread(h_pc: &mut HPCON) -> Result<STARTUPINFOEXW> {
    unsafe {
        let mut start_info = STARTUPINFOEXW::default();
        start_info.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;

        let mut lp_size: usize = 0;

        let success = InitializeProcThreadAttributeList(None, 1, None, &mut lp_size);
        // Note: This initial call will return an error by design. This is expected behavior.
        if success.is_ok() || lp_size == 0 {
            return Err(Error::from_win32());
        }

        let lp_attribute_list: Box<[u8]> = vec![0; lp_size].into_boxed_slice();
        // Need to leak this.
        let lp_attribute_list = Box::leak(lp_attribute_list);

        start_info.lpAttributeList =
            LPPROC_THREAD_ATTRIBUTE_LIST(lp_attribute_list.as_mut_ptr().cast::<_>());

        let success = InitializeProcThreadAttributeList(
            Some(start_info.lpAttributeList),
            1,
            None,
            &mut lp_size,
        );

        if !success.is_ok() {
            return Err(Error::from_win32());
        }

        let success = UpdateProcThreadAttribute(
            start_info.lpAttributeList,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            Some(h_pc.0 as _),
            std::mem::size_of_val(h_pc),
            None,
            None,
        );

        if !success.is_ok() {
            return Err(Error::from_win32());
        }

        Ok(start_info)
    }
}

unsafe fn run_process(
    startup_info: &mut STARTUPINFOEXW,
    command: &str,
    working_dir: &str,
) -> Result<PROCESS_INFORMATION> {
    unsafe {
        let mut p_info = PROCESS_INFORMATION::default();

        let success = CreateProcessW(
            PCWSTR::default(),
            Some(PWSTR(
                command
                    .encode_utf16()
                    .chain(::std::iter::once(0))
                    .collect::<Vec<u16>>()
                    .as_mut_ptr(),
            )),
            None,
            None,
            false,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            None,
            PCWSTR(HSTRING::from(working_dir).as_ptr()),
            &mut startup_info.StartupInfo,
            &mut p_info,
        );

        if !success.is_ok() {
            return Err(Error::from_win32());
        }

        Ok(p_info)
    }
}
