//! ANSI styling for non-interactive CLI output.
//!
//! Colour is enabled only for a terminal and disabled by `NO_COLOR`. Pipe
//! targets, including `list --json`, always receive plain text.

use std::io::{self, IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const RESET: &str = "\x1b[0m";
const CLEAR_LINE: &str = "\x1b[2K";
const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// True when ANSI colour should be written to stdout.
pub fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        if std::env::var_os("CLICOLOR_FORCE").is_some() {
            return true;
        }
        io::stdout().is_terminal()
    })
}

fn paint(text: &str, code: &str) -> String {
    if enabled() {
        format!("\x1b[{code}m{text}{RESET}")
    } else {
        text.to_string()
    }
}

pub fn accent(text: &str) -> String {
    paint(text, "1;96")
}

pub fn success(text: &str) -> String {
    paint(text, "32")
}

pub fn warn(text: &str) -> String {
    paint(text, "33")
}

pub fn error(text: &str) -> String {
    paint(text, "31")
}

pub fn muted(text: &str) -> String {
    paint(text, "90")
}

pub fn good(text: &str) -> String {
    paint(text, "92")
}

pub fn check() -> &'static str {
    if enabled() { "✓" } else { "ok" }
}

pub fn cross() -> &'static str {
    if enabled() { "✗" } else { "no" }
}

/// An animated spinner written to stderr, stopped by `finish` or on drop.
pub struct Spinner {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    pub fn start(label: impl Into<String>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        if !enabled() || !io::stderr().is_terminal() {
            return Self { stop, thread: None };
        }
        let flag = Arc::clone(&stop);
        let label = label.into();
        let thread = std::thread::spawn(move || {
            let mut frame = 0usize;
            while !flag.load(Ordering::Relaxed) {
                let _ = write!(
                    io::stderr(),
                    "\r  {} {}",
                    FRAMES[frame % FRAMES.len()],
                    label
                );
                let _ = io::stderr().flush();
                frame += 1;
                std::thread::sleep(Duration::from_millis(80));
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }

    pub fn finish(mut self, message: &str) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if enabled() && io::stderr().is_terminal() {
            eprint!("\r{CLEAR_LINE}  {} {}\n", success(check()), message);
        } else {
            eprintln!("  {message}");
        }
        std::mem::forget(self);
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if enabled() && io::stderr().is_terminal() {
            let _ = write!(io::stderr(), "\r{CLEAR_LINE}");
            let _ = io::stderr().flush();
        }
    }
}
