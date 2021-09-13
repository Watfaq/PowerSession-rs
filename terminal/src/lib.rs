#[cfg_attr(windows, path = "windows/mod.rs")]
mod windows;
pub use crate::windows::terminal::WindowsTerminal;

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

pub trait Terminal {
    fn run(&mut self, command: &str) -> u32;
    fn attach_stdin(&self, rx: Receiver<(Arc<[u8; 1024]>, usize)>);
    fn attach_stdout(&self, tx: Sender<(Arc<[u8; 1024]>, usize)>);
}
