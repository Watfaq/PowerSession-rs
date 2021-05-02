use std::fs::File;
use std::io::{Read, Write};

#[cfg_attr(unix, path = "linux.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod terminal;
use std::sync::mpsc::{Receiver, Sender};
pub use terminal::WindowsTerminal;

pub trait Terminal {
    fn run(&self, command: &str);
    fn attach_stdin(&self, rx: Receiver<u8>);
    fn attach_stdout(&self, tx: Sender<u8>);
}
