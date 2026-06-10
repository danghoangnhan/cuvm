//! `indicatif`-backed `ProgressReporter` for the CLI. Interactive (stderr is a
//! TTY) renders byte bars; non-interactive prints a single `Downloading …` line
//! per artifact. All output is on stderr; nothing touches stdout.

use std::collections::HashMap;
use std::io::IsTerminal;
use std::sync::Mutex;

use cuvm_download::{ProgressReporter, Reporter};
use indicatif::{ProgressBar, ProgressStyle};

/// CLI progress reporter. Cheap to construct; safe to share across threads.
pub struct CliReporter {
    interactive: bool,
    bars: Mutex<HashMap<String, ProgressBar>>,
}

impl CliReporter {
    /// Build a reporter, auto-detecting whether stderr is a terminal.
    #[must_use]
    pub fn new() -> Self {
        Self::with_interactive(std::io::stderr().is_terminal())
    }

    /// Build a reporter with interactivity pinned. Tests use this so they never
    /// depend on whether the test runner's stderr is a TTY; `new` auto-detects.
    #[must_use]
    pub fn with_interactive(interactive: bool) -> Self {
        Self {
            interactive,
            bars: Mutex::new(HashMap::new()),
        }
    }

    /// As a shared `Reporter` handle for the installer.
    #[must_use]
    pub fn shared() -> Reporter {
        std::sync::Arc::new(Self::new())
    }
}

impl Default for CliReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressReporter for CliReporter {
    fn on_download_start(&self, label: &str, total_bytes: Option<u64>) {
        if self.interactive {
            let bar = match total_bytes {
                Some(len) => {
                    let pb = ProgressBar::new(len);
                    pb.set_style(
                        ProgressStyle::with_template("{msg:>24} [{bar:30}] {bytes}/{total_bytes}")
                            .unwrap()
                            .progress_chars("=> "),
                    );
                    pb
                }
                None => ProgressBar::new_spinner(),
            };
            bar.set_message(label.to_string());
            self.bars.lock().unwrap().insert(label.to_string(), bar);
        } else {
            // Integer MiB avoids clippy::cast_precision_loss under -D warnings.
            let size =
                total_bytes.map_or_else(String::new, |b| format!(" ({} MiB)", b / (1024 * 1024)));
            eprintln!("Downloading {label}{size}");
        }
    }

    fn on_download_advance(&self, label: &str, delta_bytes: u64) {
        if self.interactive {
            if let Some(bar) = self.bars.lock().unwrap().get(label) {
                bar.inc(delta_bytes);
            }
        }
    }

    fn on_download_finish(&self, label: &str) {
        if let Some(bar) = self.bars.lock().unwrap().remove(label) {
            bar.finish_and_clear();
        }
    }

    fn on_download_abort(&self, label: &str) {
        // Same teardown as finish: a failed download must never leave a dangling
        // bar to garble the next stderr line (spec §6.4).
        if let Some(bar) = self.bars.lock().unwrap().remove(label) {
            bar.finish_and_clear();
        }
    }

    fn on_phase(&self, phase: &str) {
        // Spec §5.4: the Verifying/Extracting ticks appear in interactive AND
        // plain (redirected/CI) output.
        eprintln!("{phase}");
    }
}

/// Wrap `s` in ANSI dim (`ESC[2m … ESC[0m`) when stderr is a TTY; return it
/// unchanged otherwise (spec §6.4: styling degrades to plain when stderr is
/// not a terminal).
pub(crate) fn dim(s: &str) -> String {
    dim_if(s, std::io::stderr().is_terminal())
}

/// Pure core of [`dim`], deterministic under test.
fn dim_if(s: &str, tty: bool) -> String {
    if tty {
        format!("\x1b[2m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_interactive_full_event_cycle_does_not_panic() {
        // Interactivity is pinned (not auto-detected) so the test exercises the
        // plain path deterministically, even when a developer's stderr is a TTY.
        let r = CliReporter::with_interactive(false);
        r.on_download_start("cuda_cudart 12.4.131", Some(25_000_000));
        r.on_download_advance("cuda_cudart 12.4.131", 1024);
        r.on_download_finish("cuda_cudart 12.4.131");
        r.on_download_abort("cuda_cudart 12.4.131");
        r.on_phase("Extracting");
        assert!(r.bars.lock().unwrap().is_empty());
    }

    #[test]
    fn interactive_abort_clears_the_tracked_bar() {
        let r = CliReporter::with_interactive(true);
        r.on_download_start("cuda_cudart 12.4.131", Some(1024));
        assert_eq!(r.bars.lock().unwrap().len(), 1);
        r.on_download_abort("cuda_cudart 12.4.131");
        assert!(
            r.bars.lock().unwrap().is_empty(),
            "abort must tear the bar down like finish does"
        );
    }

    #[test]
    fn dim_is_plain_when_not_a_tty_and_wrapped_when_tty() {
        // Only the pure core is asserted: `dim` itself depends on the runner's
        // stderr, which is not deterministic under `cargo test`.
        assert_eq!(
            dim_if("Installed CUDA 12.4.1 in 8.3s", false),
            "Installed CUDA 12.4.1 in 8.3s"
        );
        assert_eq!(dim_if("x", true), "\x1b[2mx\x1b[0m");
    }
}
