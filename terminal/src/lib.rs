#[cfg_attr(windows, path = "windows/mod.rs")]
mod windows;
pub use crate::windows::terminal::WindowsTerminal;

use std::sync::mpsc::{Receiver, Sender};

pub trait Terminal {
    fn run(&mut self, command: &str) -> u32;
    fn attach_stdin(&self, rx: Receiver<u8>);
    fn attach_stdout(&self, tx: Sender<u8>);
}
