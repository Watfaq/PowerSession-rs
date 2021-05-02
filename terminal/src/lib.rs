use std::fs::File;

#[cfg_attr(unix, path = "linux.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod terminal;
pub use terminal::WindowsTerminal;

pub trait Terminal {
    fn run(&self, command: &str);
    fn get_stdin(&self) -> File;
    fn get_stdout(&self) -> File;
}
