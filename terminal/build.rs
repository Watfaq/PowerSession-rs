fn main() {
    #[cfg(windows)]
    windows::build!(
        Windows::Win32::WindowsProgramming::{
            CloseHandle,
            STD_HANDLE_TYPE,
            GetStdHandle,
            PROCESS_CREATION_FLAGS,
        },
        Windows::Win32::SystemServices::{
            ClosePseudoConsole, CreatePseudoConsole,
            CreatePipe,
            DeleteProcThreadAttributeList,
            GetConsoleMode, SetConsoleMode,
            HANDLE, INVALID_HANDLE_VALUE,
            HPCON, COORD, CONSOLE_SCREEN_BUFFER_INFO, GetConsoleScreenBufferInfo,
            STARTUPINFOEXW,STARTUPINFOW, PROCESS_INFORMATION, STARTUPINFOW_FLAGS,SECURITY_ATTRIBUTES,
            CreateProcessW, InitializeProcThreadAttributeList, UpdateProcThreadAttribute,LPPROC_THREAD_ATTRIBUTE_LIST,
            LPPROC_THREAD_ATTRIBUTE_LIST, BOOL
        },

        Windows::Win32::FileSystem::{WriteFile, ReadFile},
        Windows::Win32::Debug::GetLastError,
    );
}
