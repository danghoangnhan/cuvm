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
        Self {
            interactive: std::io::stderr().is_terminal(),
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

    fn on_phase(&self, phase: &str) {
        if self.interactive {
            eprintln!("{phase}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_interactive_start_emits_a_line_without_panicking() {
        // In `cargo test`, stderr is not a TTY, so this exercises the plain path.
        let r = CliReporter::new();
        assert!(!r.interactive);
        r.on_download_start("cuda_cudart 12.4.131", Some(25_000_000));
        r.on_download_advance("cuda_cudart 12.4.131", 1024);
        r.on_download_finish("cuda_cudart 12.4.131");
        r.on_phase("Extracting");
    }
}
