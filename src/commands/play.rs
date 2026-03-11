use crate::commands::types::{LineItem, RecordHeader, SessionLine, V1Recording};

use std::collections::HashMap;
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

#[cfg(windows)]
struct ConsoleGuard {
    handle: HANDLE,
    original_mode: CONSOLE_MODE,
}

#[cfg(windows)]
impl Drop for ConsoleGuard {
    fn drop(&mut self) {
        unsafe {
            // Best-effort restoration; log any failure but don't panic
            if let Err(e) = SetConsoleMode(self.handle, self.original_mode) {
                eprintln!("Warning: failed to restore console mode: {:?}", e);
            }
        }
    }
}

enum SessionLineSource {
    Lines(io::Lines<Box<dyn BufRead>>),
    Vec(std::vec::IntoIter<SessionLine>),
}

struct Session {
    #[allow(dead_code)]
    header: RecordHeader,
    line_iter: SessionLineSource,
}

struct StdoutIter(Session);

impl Iterator for StdoutIter {
    type Item = SessionLine;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0.line_iter {
            SessionLineSource::Vec(iter) => iter.next(),
            SessionLineSource::Lines(iter) => loop {
                let line = iter.next()?;
                let content = match line {
                    Ok(l) => l,
                    Err(e) => {
                        eprintln!("error reading session data: {}", e);
                        exit(1);
                    }
                };
                // Skip empty or whitespace-only lines (e.g. trailing newlines in files)
                if content.trim().is_empty() {
                    continue;
                }
                let line_data: Vec<LineItem> = match serde_json::from_str(&content) {
                    Ok(data) => data,
                    Err(e) => {
                        eprintln!("corrupt record data: {}", e);
                        exit(1);
                    }
                };
                if line_data.len() != 3 {
                    eprintln!("corrupt record: expected 3 fields, got {}", line_data.len());
                    exit(1);
                }

                let session_line = SessionLine {
                    timestamp: match &line_data[0] {
                        LineItem::F64(ts) => ts.clone(),
                        _ => {
                            eprintln!("corrupt record: expected timestamp as number");
                            exit(1);
                        }
                    },
                    stdout: match &line_data[1] {
                        LineItem::String(flag) => flag == "o",
                        _ => {
                            eprintln!("corrupt record: expected event type as string");
                            exit(1);
                        }
                    },
                    content: match &line_data[2] {
                        LineItem::String(line) => line.clone(),
                        _ => {
                            eprintln!("corrupt record: expected content as string");
                            exit(1);
                        }
                    },
                };
                return Some(session_line);
            },
        }
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

fn is_url(input: &str) -> bool {
    input.starts_with("http://") || input.starts_with("https://")
}

/// Normalize an asciinema.org recording URL to its raw `.cast` download URL.
/// For example, `https://asciinema.org/a/abc123` becomes
/// `https://asciinema.org/a/abc123.cast`.
/// Query strings and fragments are preserved (e.g. `?t=10` stays after `.cast`).
fn normalize_url(url: &str) -> String {
    // Split off fragment, preserving the leading '#'
    let (before_fragment, fragment) = match url.find('#') {
        Some(idx) => (&url[..idx], &url[idx..]),
        None => (url, ""),
    };

    // Split off query, preserving the leading '?'
    let (mut main, query) = match before_fragment.find('?') {
        Some(idx) => (&before_fragment[..idx], &before_fragment[idx..]),
        None => (before_fragment, ""),
    };

    // Only normalize asciinema recording URLs
    if main.contains("asciinema.org/a/") {
        // Remove a trailing slash from the path, if present
        if main.ends_with('/') {
            main = &main[..main.len() - 1];
        }

        let mut normalized = main.to_string();
        if !normalized.ends_with(".cast") {
            normalized.push_str(".cast");
        }

        // Reattach query and fragment in their original order
        normalized.push_str(query);
        normalized.push_str(fragment);
        normalized
    } else {
        // Non-asciinema URLs are returned unchanged
        url.to_string()
    }
}

/// Parse a session from a buffered reader, detecting v2 or v1 format automatically.
/// The `source_name` is used only in error messages.
fn parse_reader(reader: Box<dyn BufRead>, source_name: &str) -> Session {
    let mut line_iter: io::Lines<Box<dyn BufRead>> = reader.lines();

    let first_line = match line_iter.next() {
        Some(Ok(line)) => line,
        Some(Err(e)) => {
            eprintln!("error reading '{}': {}", source_name, e);
            exit(1);
        }
        None => {
            eprintln!("'{}': file is empty", source_name);
            exit(1);
        }
    };

    if let Ok(header) = serde_json::from_str::<RecordHeader>(&first_line) {
        // v2 format: header on first line, events stream on subsequent lines.
        // Validate version == 2 to avoid misclassifying v1 recordings that
        // happen to contain a timestamp field parseable as RecordHeader.
        if header.version == 2 {
            Session {
                header,
                line_iter: SessionLineSource::Lines(line_iter),
            }
        } else {
            // Not v2 — fall through and try v1 parsing with the full content.
            let mut file_content = first_line;
            for line in line_iter {
                file_content.push('\n');
                match line {
                    Ok(l) => file_content.push_str(&l),
                    Err(e) => {
                        eprintln!("error reading '{}': {}", source_name, e);
                        exit(1);
                    }
                }
            }
            parse_v1(source_name, file_content)
        }
    } else {
        // Try v1 format: entire content is a single JSON object.
        // Collect remaining lines from the already-opened iterator.
        let mut file_content = first_line;
        for line in line_iter {
            file_content.push('\n');
            match line {
                Ok(l) => file_content.push_str(&l),
                Err(e) => {
                    eprintln!("error reading '{}': {}", source_name, e);
                    exit(1);
                }
            }
        }
        parse_v1(source_name, file_content)
    }
}

fn parse_v1(source_name: &str, file_content: String) -> Session {
    match serde_json::from_str::<V1Recording>(&file_content) {
        Ok(recording) if recording.version == 1 => {
            let header = RecordHeader {
                version: recording.version,
                width: recording.width,
                height: recording.height,
                timestamp: 0,
                environment: HashMap::new(),
            };

            let mut absolute_time: f64 = 0.0;
            let events: Vec<SessionLine> = recording
                .stdout
                .into_iter()
                .map(|(delay, text)| {
                    absolute_time += delay;
                    SessionLine {
                        timestamp: absolute_time,
                        stdout: true,
                        content: text,
                    }
                })
                .collect();

            Session {
                header,
                line_iter: SessionLineSource::Vec(events.into_iter()),
            }
        }
        Ok(recording) => {
            eprintln!(
                "'{}': unsupported file format version {}",
                source_name, recording.version
            );
            exit(1);
        }
        Err(e) => {
            eprintln!(
                "'{}': unsupported or corrupt session file: {}",
                source_name, e
            );
            exit(1);
        }
    }
}

impl Session {
    fn new(source: &str) -> Self {
        if is_url(source) { Self::from_url(source) } else { Self::from_file(source) }
    }

    fn from_file(filename: &str) -> Self {
        if !Path::new(filename).exists() {
            eprintln!("session with name {} does not exist", filename);
            exit(1);
        }

        let file = match File::open(filename) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("error opening '{}': {}", filename, e);
                exit(1);
            }
        };
        parse_reader(Box::new(io::BufReader::new(file)), filename)
    }

    fn from_url(url: &str) -> Self {
        let url = normalize_url(url);
        let response = reqwest::blocking::get(&url).unwrap_or_else(|e| {
            eprintln!("failed to fetch URL {}: {}", url, e);
            exit(1);
        });

        if !response.status().is_success() {
            eprintln!("failed to fetch URL {}: HTTP {}", url, response.status());
            exit(1);
        }

        parse_reader(Box::new(io::BufReader::new(response)), &url)
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
        // We use an RAII guard to ensure console mode is restored on all exit paths.
        #[cfg(windows)]
        let _console_guard: Option<ConsoleGuard> = unsafe {
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
                        if let Err(e) = SetConsoleMode(h, raw) {
                            warn!("pause: failed to set console mode: {:?}; space-to-pause is disabled", e);
                            None
                        } else {
                            Some(ConsoleGuard {
                                handle: h,
                                original_mode: orig,
                            })
                        }
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
        // Only spawn the thread when pause support is actually enabled.
        #[cfg(windows)]
        if _console_guard.is_some() {
            thread::spawn(move || {
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
            });
        }

        #[cfg(not(windows))]
        {
            use std::io::IsTerminal;
            // Only enable pause when stdin is an interactive terminal; don't
            // consume piped input in tests or scripted environments.
            if std::io::stdin().is_terminal() {
                thread::spawn(move || {
                    use std::io::Read;
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
                });
            }
        }

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
        // Console mode is automatically restored by the ConsoleGuard's Drop impl
    }
}

#[cfg(test)]
mod tests {
    use super::{is_url, normalize_url, wait_interruptible};
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

    fn test_data_v1_path() -> String {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("testdata/play_v1.json");
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

    #[test]
    fn test_play_v1_format() {
        let play = Play::new(test_data_v1_path(), None, 1.0);
        play.execute();
    }

    #[test]
    fn test_play_v1_format_with_speed() {
        let play = Play::new(test_data_v1_path(), None, 2.0);
        play.execute();
    }

    #[test]
    fn test_play_v1_format_with_idle_time_limit() {
        let play = Play::new(test_data_v1_path(), Some(0.5), 1.0);
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

    #[test]
    fn test_is_url() {
        assert!(is_url("https://asciinema.org/a/abc123"));
        assert!(is_url("http://example.com/recording.cast"));
        assert!(!is_url("my_recording.cast"));
        assert!(!is_url("/path/to/recording.cast"));
        assert!(!is_url("C:\\recordings\\session.cast"));
    }

    #[test]
    fn test_normalize_url_asciinema() {
        // Basic recording URL
        assert_eq!(
            normalize_url("https://asciinema.org/a/abc123"),
            "https://asciinema.org/a/abc123.cast"
        );
        // Already has .cast – should not double-append
        assert_eq!(
            normalize_url("https://asciinema.org/a/abc123.cast"),
            "https://asciinema.org/a/abc123.cast"
        );
        // Non-asciinema URL – should be returned unchanged
        assert_eq!(
            normalize_url("https://example.com/recording.cast"),
            "https://example.com/recording.cast"
        );
        // URL with query string – .cast inserted before '?'
        assert_eq!(
            normalize_url("https://asciinema.org/a/abc123?t=10"),
            "https://asciinema.org/a/abc123.cast?t=10"
        );
        // URL with trailing slash – slash stripped before appending .cast
        assert_eq!(
            normalize_url("https://asciinema.org/a/abc123/"),
            "https://asciinema.org/a/abc123.cast"
        );
        // URL with fragment – .cast inserted before '#'
        assert_eq!(
            normalize_url("https://asciinema.org/a/abc123#intro"),
            "https://asciinema.org/a/abc123.cast#intro"
        );
        // URL with both query string and fragment
        assert_eq!(
            normalize_url("https://asciinema.org/a/abc123?t=10#intro"),
            "https://asciinema.org/a/abc123.cast?t=10#intro"
        );
    }
}
