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
            GetConsoleMode, SetConsoleMode,
            HANDLE, INVALID_HANDLE_VALUE,
            HPCON, COORD, CONSOLE_SCREEN_BUFFER_INFO, GetConsoleScreenBufferInfo,
            STARTUPINFOW, PROCESS_INFORMATION, STARTUPINFOW_FLAGS,
            CreateProcessW, InitializeProcThreadAttributeList, UpdateProcThreadAttribute
        },

        Windows::Win32::FileSystem::{WriteFile, ReadFile},
        Windows::Win32::Debug::GetLastError,
    );
}
