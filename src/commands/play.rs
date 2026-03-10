use crate::commands::types::{LineItem, RecordHeader, SessionLine};

use std::fs::File;
use std::io;
use std::io::{BufRead, Write};

use std::path::Path;
use std::process::exit;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use log::warn;

#[cfg(windows)]
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::ReadFile,
    System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, CONSOLE_MODE, ENABLE_ECHO_INPUT,
        ENABLE_LINE_INPUT, STD_INPUT_HANDLE,
    },
};

struct Session {
    #[allow(dead_code)]
    header: RecordHeader,
    line_iter: io::Lines<io::BufReader<File>>,
}

struct StdoutIter(Session);

impl Iterator for StdoutIter {
    type Item = SessionLine;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.line_iter.next().map(|line| {
            let content = line.unwrap();
            let line_data: Vec<LineItem> = serde_json::from_str(&content).unwrap();
            if line_data.len() != 3 {
                panic!("invalid record data");
            }

            SessionLine {
                timestamp: match &line_data[0] {
                    LineItem::F64(ts) => ts.clone(),
                    _ => {
                        panic!("corrupt record");
                    }
                },
                stdout: match &line_data[1] {
                    LineItem::String(flag) => flag == "o",
                    _ => {
                        panic!("corrupt record");
                    }
                },
                content: match &line_data[2] {
                    LineItem::String(line) => line.clone(),
                    _ => {
                        panic!("corrupt record");
                    }
                },
            }
        })
    }
}

struct StdoutRelativeTimeIter(StdoutIter, f64);

impl Iterator for StdoutRelativeTimeIter {
    type Item = SessionLine;

    fn next(&mut self) -> Option<Self::Item> {
        let prev_timestamp = self.1;

        self.0.next().map(|line| {
            let rv = SessionLine {
                timestamp: match prev_timestamp {
                    x if x == 0.0 => 0.0, // first line, start right away
                    _ => line.timestamp - prev_timestamp,
                },
                content: line.content,
                stdout: line.stdout,
            };
            self.1 = line.timestamp;
            rv
        })
    }
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

impl Session {
    fn new(filename: &str) -> Self {
        if !Path::new(filename).exists() {
            println!("session with name {} does not exist", filename);
            exit(1);
        }

        let mut line_iter = read_lines(filename).unwrap();
        let header_line = line_iter.next().unwrap();
        let header: RecordHeader = serde_json::from_str(header_line.unwrap().as_str()).unwrap();
        Session { header, line_iter }
    }

    fn stdout_iter(self) -> StdoutIter {
        StdoutIter(self)
    }

    fn stdout_relative_time_iter(self) -> StdoutRelativeTimeIter {
        StdoutRelativeTimeIter(self.stdout_iter(), 0.0)
    }
}

/// Waits for `delay` seconds, respecting pause/resume signals from `pair`.
/// Returns once the full delay has elapsed (accounting for time spent paused).
fn wait_interruptible(pair: &Arc<(Mutex<bool>, Condvar)>, delay: f64) {
    let mut remaining = delay;
    while remaining > 1e-9 {
        let (lock, cvar) = &**pair;
        let mut paused_guard = lock.lock().unwrap();

        // Block while paused.
        while *paused_guard {
            paused_guard = cvar.wait(paused_guard).unwrap();
        }

        // Wait for the remaining delay; an early wakeup means pause was toggled.
        let start = Instant::now();
        let (_guard, timed_out) = cvar
            .wait_timeout(paused_guard, Duration::from_secs_f64(remaining))
            .unwrap();

        let elapsed = start.elapsed().as_secs_f64();
        remaining = (remaining - elapsed).max(0.0);

        if timed_out.timed_out() {
            break;
        }
        // Woken early because pause was toggled — loop back to re-check
        // the paused state and continue with the remaining delay.
    }
}

pub struct Play {
    session: Session,
    idle_time_limit: Option<f64>,
    speed: f64,
}

impl Play {
    pub fn new(filename: String, idle_time_limit: Option<f64>, speed: f64) -> Self {
        Play {
            session: Session::new(&filename),
            idle_time_limit,
            speed,
        }
    }

    pub fn execute(self) {
        // Shared pause state: (paused flag, condvar to signal changes)
        let pair = Arc::new((Mutex::new(false), Condvar::new()));
        let pair_clone = Arc::clone(&pair);

        // On Windows: save stdin console mode and switch to raw (no line
        // buffering / no echo) so space arrives immediately without Enter.
        // We manage this in the *main* thread so the restore is guaranteed to
        // run when execute() returns, even if the reader thread is still
        // blocked inside ReadFile.
        #[cfg(windows)]
        let stdin_restore: Option<(isize, u32)> = unsafe {
            match GetStdHandle(STD_INPUT_HANDLE) {
                Ok(h) if !h.is_invalid() => {
                    let mut orig = CONSOLE_MODE::default();
                    if GetConsoleMode(h, &mut orig).is_err() {
                        warn!(
                            "pause: stdin is not a console (redirected?); \
                             space-to-pause is disabled"
                        );
                        None
                    } else {
                        let mut raw = orig;
                        raw &= !ENABLE_LINE_INPUT;
                        raw &= !ENABLE_ECHO_INPUT;
                        SetConsoleMode(h, raw).ok();
                        Some((h.0 as isize, orig.0))
                    }
                }
                _ => {
                    warn!(
                        "pause: could not obtain a valid stdin handle; \
                         space-to-pause is disabled"
                    );
                    None
                }
            }
        };

        // Spawn a thread that reads stdin and toggles pause when space is pressed.
        // Console mode is already configured by the main thread above.
        thread::spawn(move || {
            #[cfg(windows)]
            unsafe {
                let stdin_handle = match GetStdHandle(STD_INPUT_HANDLE) {
                    Ok(h) if !h.is_invalid() => h,
                    _ => return,
                };
                loop {
                    let mut buf = [0u8; 1];
                    let mut n_read: u32 = 0;
                    if ReadFile(stdin_handle, Some(&mut buf), Some(&mut n_read), None).is_err()
                        || n_read == 0
                    {
                        break;
                    }
                    if buf[0] == b' ' {
                        let (lock, cvar) = &*pair_clone;
                        let mut paused = lock.lock().unwrap();
                        *paused = !*paused;
                        cvar.notify_all();
                    }
                }
            }

            #[cfg(not(windows))]
            {
                use std::io::{IsTerminal, Read};
                // Only enable pause when stdin is an interactive terminal; don't
                // consume piped input in tests or scripted environments.
                if !std::io::stdin().is_terminal() {
                    return;
                }
                loop {
                    let mut buf = [0u8; 1];
                    match std::io::stdin().lock().read(&mut buf) {
                        Ok(1) if buf[0] == b' ' => {
                            let (lock, cvar) = &*pair_clone;
                            let mut paused = lock.lock().unwrap();
                            *paused = !*paused;
                            cvar.notify_all();
                        }
                        Ok(n) if n > 0 => {}
                        _ => break,
                    }
                }
            }
        });

        for stdout_item in self.session.stdout_relative_time_iter() {
            let mut delay = stdout_item.timestamp;
            if let Some(limit) = self.idle_time_limit {
                delay = delay.min(limit);
            }
            delay /= self.speed;

            wait_interruptible(&pair, delay);

            io::stdout()
                .write_all(stdout_item.content.as_bytes())
                .unwrap();
            io::stdout().flush().unwrap();
        }

        // Restore the original console mode now that playback is done.
        #[cfg(windows)]
        unsafe {
            if let Some((handle_val, mode_val)) = stdin_restore {
                SetConsoleMode(HANDLE(handle_val as _), CONSOLE_MODE(mode_val)).ok();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::wait_interruptible;
    use crate::Play;
    use std::path::PathBuf;
    use std::sync::{Arc, Condvar, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    fn test_data_path() -> String {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("testdata/play.txt");
        d.as_path().to_str().unwrap().to_owned()
    }

    #[test]
    fn test_play() {
        let play = Play::new(test_data_path(), None, 1.0);
        play.execute();
    }

    #[test]
    fn test_play_with_speed() {
        let play = Play::new(test_data_path(), None, 2.0);
        play.execute();
    }

    #[test]
    fn test_play_with_idle_time_limit() {
        let play = Play::new(test_data_path(), Some(0.5), 1.0);
        play.execute();
    }

    #[test]
    fn test_play_with_speed_and_idle_time_limit() {
        let play = Play::new(test_data_path(), Some(0.5), 2.0);
        play.execute();
    }

    /// Verify that wait_interruptible completes without pause after the
    /// requested delay.
    #[test]
    fn test_wait_interruptible_no_pause() {
        let pair = Arc::new((Mutex::new(false), Condvar::new()));
        let start = Instant::now();
        wait_interruptible(&pair, 0.05);
        assert!(start.elapsed() >= Duration::from_millis(30));
    }

    /// Verify that wait_interruptible blocks while paused and resumes correctly
    /// once the pause is lifted, accounting for the time spent paused.
    #[test]
    fn test_wait_interruptible_pause_and_resume() {
        let pair = Arc::new((Mutex::new(false), Condvar::new()));
        let pair_clone = Arc::clone(&pair);

        // Start paused immediately.
        {
            let (lock, cvar) = &*pair;
            let mut paused = lock.lock().unwrap();
            *paused = true;
            cvar.notify_all();
        }

        // Lift the pause after 50 ms.
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            let (lock, cvar) = &*pair_clone;
            let mut paused = lock.lock().unwrap();
            *paused = false;
            cvar.notify_all();
        });

        let start = Instant::now();
        // Tiny delay — execution is dominated by the 50 ms pause period.
        wait_interruptible(&pair, 0.001);
        let elapsed = start.elapsed();

        // Must have waited at least ~50 ms for the resume signal.
        assert!(elapsed >= Duration::from_millis(30));
    }
}
