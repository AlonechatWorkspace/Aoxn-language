//! Terminal UX: color, TTY detection, progress, non-TTY degradation.
//!
//! Everything user-visible goes through [`Ui`] so that:
//! - non-TTY (CI logs, pipes) automatically loses color and step markers,
//! - `--no-color` forces plain output,
//! - `--json` suppresses human chatter (commands emit machine output instead).
//!
//! Color is hand-rolled ANSI (the project tradition is no heavy deps): the
//! standard library's `IsTerminal` detects a TTY, `NO_COLOR`/`--no-color`
//! opt out, and modern Windows terminals handle ANSI natively.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub struct Ui {
    color: bool,
    tty: bool,
    json: bool,
    quiet: bool,
    /// set while a step is live so later lines can clear it
    spinning: AtomicBool,
}

static CURRENT: std::sync::Mutex<Option<Ui>> = std::sync::Mutex::new(None);

/// Install the process-wide UI (called once from `dispatch`).
pub fn init(no_color: bool, json: bool, quiet: bool) {
    let tty = std::io::stderr().is_terminal();
    let color = tty && !no_color && std::env::var_os("NO_COLOR").is_none();
    *CURRENT.lock().unwrap() = Some(Ui {
        color,
        tty,
        json,
        quiet,
        spinning: AtomicBool::new(false),
    });
}

impl Clone for Ui {
    fn clone(&self) -> Self {
        Ui {
            color: self.color,
            tty: self.tty,
            json: self.json,
            quiet: self.quiet,
            spinning: AtomicBool::new(false),
        }
    }
}

/// Clone of the process-wide UI (or a plain non-TTY default before init).
pub fn current() -> Ui {
    CURRENT
        .lock()
        .unwrap()
        .clone()
        .unwrap_or(Ui {
            color: false,
            tty: false,
            json: false,
            quiet: false,
            spinning: AtomicBool::new(false),
        })
}

// ANSI escape helpers (no-ops when color is off)
fn color_enabled() -> bool {
    std::io::stderr().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
        && CURRENT.lock().unwrap().as_ref().map_or(true, |u| u.color)
}

fn wrap(codes: &str, s: &str) -> String {
    if color_enabled() {
        format!("\x1b[{codes}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn style_err(s: &str) -> String {
    wrap("1;31", s)
}

pub fn style_warn(s: &str) -> String {
    wrap("1;33", s)
}

pub fn style_ok(s: &str) -> String {
    wrap("1;32", s)
}

pub fn style_hint(s: &str) -> String {
    wrap("1;36", s)
}

pub fn style_dim(s: &str) -> String {
    wrap("2", s)
}

fn clear_line() {
    if std::io::stderr().is_terminal() {
        eprint!("\x1b[2K\r");
    }
}

impl Ui {
    /// Regular progress line (suppressed in --json / --quiet mode).
    pub fn info(&self, msg: &str) {
        if !self.json && !self.quiet {
            eprintln!("{msg}");
        }
    }

    /// Always-visible notice (warnings survive --quiet but not --json).
    pub fn warn(&self, msg: &str) {
        if !self.json {
            eprintln!("{} {msg}", style_warn("warning:"));
        }
    }

    /// Final success line (stdout, so it can be piped).
    pub fn success(&self, msg: &str) {
        if !self.json && !self.quiet {
            println!("{}", style_ok(msg));
        }
    }

    /// Plain stdout line for command output (tables, trees) — never colored,
    /// never suppressed: this IS the command's result.
    pub fn out(&self, msg: &str) {
        println!("{msg}");
    }

    /// A step marker: live `· label` line on a TTY (closed by done_*), or a
    /// plain `· label ...` line followed by the result otherwise.
    pub fn step(&self, label: &str) -> Step<'_> {
        if self.json || self.quiet {
            return Step {
                ui: self,
                label: String::new(),
                live: false,
                start: Instant::now(),
            };
        }
        if self.tty {
            eprint!("{} {label}", style_dim("·"));
            self.spinning.store(true, Ordering::Relaxed);
            Step {
                ui: self,
                label: label.to_string(),
                live: true,
                start: Instant::now(),
            }
        } else {
            eprintln!("{} {label} ...", style_dim("·"));
            Step {
                ui: self,
                label: label.to_string(),
                live: false,
                start: Instant::now(),
            }
        }
    }
}

/// One progress step; finishes with `done_ok` / `done_ok_msg` / `done_fail`.
pub struct Step<'a> {
    ui: &'a Ui,
    label: String,
    live: bool,
    start: Instant,
}

impl Step<'_> {
    fn suffix(&self) -> String {
        let dt = self.start.elapsed();
        if dt > Duration::from_secs(2) {
            format!(" ({:.1?})", dt)
        } else {
            String::new()
        }
    }

    pub fn done_ok(self, extra: &str) {
        let s = self.suffix();
        if self.live {
            self.ui.spinning.store(false, Ordering::Relaxed);
            clear_line();
            eprintln!("{} ok{}{}", style_ok("✓"), s, extra);
        } else if !self.label.is_empty() {
            eprintln!("  {} ok{}{}", style_ok("✓"), s, extra);
        }
    }

    pub fn done_ok_msg(self, msg: &str) {
        if self.live {
            self.ui.spinning.store(false, Ordering::Relaxed);
            clear_line();
            eprintln!("{} {msg}", style_ok("✓"));
        } else if !self.label.is_empty() {
            eprintln!("  {msg}");
        }
    }

    pub fn done_fail(self, extra: &str) {
        let s = self.suffix();
        if self.live {
            self.ui.spinning.store(false, Ordering::Relaxed);
            clear_line();
            eprintln!("{} failed{}{}", style_err("✗"), s, extra);
        } else if !self.label.is_empty() {
            eprintln!("  {} failed{}{}", style_err("✗"), s, extra);
        }
    }

}
