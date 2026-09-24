//! Terminal output: one calm line per check, a spinner only while work runs.

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::model::Sev;
use crate::text::width;

pub const LINE: usize = 64;

#[derive(Clone, Copy)]
pub struct Ui {
    pub color: bool,
    pub tty: bool,
}

impl Ui {
    pub fn new() -> Self {
        let tty = std::io::stdout().is_terminal();
        let color = tty && std::env::var_os("NO_COLOR").is_none() && crate::win::enable_vt();
        Ui { color, tty }
    }

    pub fn paint(&self, code: &str, s: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn dim(&self, s: &str) -> String {
        self.paint("2", s)
    }
    pub fn bold(&self, s: &str) -> String {
        self.paint("1", s)
    }

    pub fn sev(&self, sev: Sev, s: &str) -> String {
        let code = match sev {
            Sev::Critical => "1;31",
            Sev::Warning => "33",
            Sev::Info => "36",
            Sev::Ok => "32",
        };
        self.paint(code, s)
    }

    pub fn println(&self, s: &str) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{s}");
    }

    fn prefix(i: usize, n: usize, label: &str) -> String {
        format!("  {:>2}/{n}  {label}", i)
    }

    pub fn start(&self, i: usize, n: usize, label: &str) -> Spinner {
        let text = Self::prefix(i, n, label);
        let stop = Arc::new(AtomicBool::new(false));
        let handle = if self.tty {
            let (stop2, ui) = (stop.clone(), *self);
            Some(std::thread::spawn(move || {
                let frames = ['|', '/', '-', '\\'];
                let mut k = 0;
                while !stop2.load(Ordering::Relaxed) {
                    let mut out = std::io::stdout().lock();
                    let _ = write!(out, "\r{} {}", text, ui.dim(&frames[k % 4].to_string()));
                    let _ = out.flush();
                    drop(out);
                    k += 1;
                    std::thread::sleep(Duration::from_millis(110));
                }
            }))
        } else {
            None
        };
        Spinner { stop, handle, ui: *self, i, n, label: label.to_string() }
    }
}

pub struct Spinner {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    ui: Ui,
    i: usize,
    n: usize,
    label: String,
}

impl Spinner {
    pub fn finish(mut self, sev: Option<Sev>, summary: &str) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let left = Ui::prefix(self.i, self.n, &self.label);
        let dots = LINE.saturating_sub(width(&left) + width(summary) + 2).max(2);
        let tail = match sev {
            Some(s) => self.ui.sev(s, summary),
            None => self.ui.dim(summary),
        };
        let line = format!("{left} {} {tail}", self.ui.dim(&".".repeat(dots)));
        let clear = if self.ui.tty { "\r" } else { "" };
        // Pad instead of an erase escape so terminals without VT stay clean.
        let pad = " ".repeat(8);
        self.ui.println(&format!("{clear}{line}{pad}"));
    }
}
