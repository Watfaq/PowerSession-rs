extern crate core;

#[cfg(windows)]
mod impl_win;

#[cfg(windows)]
pub use impl_win::terminal::WindowsTerminal;
use std::error::Error;

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub trait Terminal {
    fn run(&mut self, command: &str) -> Result<u32>;
    fn attach_stdin(&self, rx: Receiver<(Arc<[u8]>, usize)>);
    fn attach_stdout(&self, tx: Sender<(Arc<[u8]>, usize)>);
}

#[cfg(test)]
mod tests {
    use crate::terminal::{Terminal, WindowsTerminal};
    use std::borrow::Borrow;
    use std::sync::mpsc::channel;
    use std::sync::Arc;
    use std::thread;

    #[test]
    #[ignore]
    fn test_terminal_stdin_stdout() {
        let mut t = WindowsTerminal::new(None);
        let (stdin_tx, stdin_rx) = channel::<(Arc<[u8]>, usize)>();
        let (stdout_tx, stdout_rx) = channel::<(Arc<[u8]>, usize)>();

        t.attach_stdin(stdin_rx);
        t.attach_stdout(stdout_tx);

        let target_text = "RaNdAmTExT";

        let main = thread::spawn(move || {
            t.run("cmd.exe").expect("should start process");
        });

        let cmd = format!("echo {}\r\nexit\r\n", target_text);

        for i in 0..cmd.as_bytes().len() {
            let mut buf = Vec::new();
            buf.resize(10, 0);
            buf[0] = cmd.as_bytes()[i];

            stdin_tx.send((Arc::from(buf), 1)).unwrap();
        }

        let mut result = vec![];

        loop {
            let (output, n) = stdout_rx.recv().unwrap();
            if n == 0 {
                break;
            }
            result.extend(&output[..n]);
        }

        let output = std::str::from_utf8(result.borrow()).unwrap();
        assert!(
            output.contains(target_text),
            "{} should contains `{}`",
            output,
            target_text
        );

        main.join().unwrap();
    }
}
