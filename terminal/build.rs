fn main() {
    #[cfg(windows)]
    windows::build!(
        Windows::Win32::System::WindowsProgramming::{
            CloseHandle,
            STD_HANDLE_TYPE,
            GetStdHandle,
            STD_OUTPUT_HANDLE,
        },
        Windows::Win32::System::Threading::{
            DeleteProcThreadAttributeList,
            PROCESS_CREATION_FLAGS,
            STARTUPINFOW,
            PROCESS_INFORMATION,
            STARTUPINFOW_FLAGS,
            LPPROC_THREAD_ATTRIBUTE_LIST,
            CreateProcessW,
            InitializeProcThreadAttributeList,
            UpdateProcThreadAttribute,
            EXTENDED_STARTUPINFO_PRESENT,
        },
        Windows::Win32::System::Console::{
            ClosePseudoConsole,
            CreatePseudoConsole,
            GetConsoleMode,
            SetConsoleMode,
            COORD,
            CONSOLE_SCREEN_BUFFER_INFO,
            GetConsoleScreenBufferInfo,
        },
        Windows::Win32::System::SystemServices::{
            CreatePipe,
            HANDLE, INVALID_HANDLE_VALUE,
            HPCON,
            STARTUPINFOEXW, SECURITY_ATTRIBUTES,
            BOOL,PWSTR
        },
        Windows::Win32::Storage::FileSystem::{WriteFile, ReadFile},
        Windows::Win32::System::Diagnostics::Debug::{
            GetLastError
        }
    );
}
