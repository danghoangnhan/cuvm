//! Progress sink for long-running download/extract work. Object-safe; the CLI
//! supplies an `indicatif`-backed impl, everything else uses `SilentReporter`.

use std::sync::Arc;

/// Receives progress callbacks during `Downloader::fetch` and installer phases.
pub trait ProgressReporter: Send + Sync {
    /// A download for `label` is starting; `total_bytes` is the content length if known.
    fn on_download_start(&self, label: &str, total_bytes: Option<u64>);
    /// `delta_bytes` more bytes of `label` have been written.
    fn on_download_advance(&self, label: &str, delta_bytes: u64);
    /// The download for `label` finished (verified + renamed).
    fn on_download_finish(&self, label: &str);
    /// A non-download phase began (e.g. "Verifying", "Extracting").
    fn on_phase(&self, phase: &str);
}

/// Shared, cheaply-cloneable handle to a reporter.
pub type Reporter = Arc<dyn ProgressReporter>;

/// No-op reporter — the default everywhere except an interactive CLI.
#[derive(Debug, Default, Clone, Copy)]
pub struct SilentReporter;

impl ProgressReporter for SilentReporter {
    fn on_download_start(&self, _: &str, _: Option<u64>) {}
    fn on_download_advance(&self, _: &str, _: u64) {}
    fn on_download_finish(&self, _: &str) {}
    fn on_phase(&self, _: &str) {}
}

/// A silent reporter as a shared handle.
#[must_use]
pub fn silent() -> Reporter {
    Arc::new(SilentReporter)
}

#[cfg(test)]
pub(crate) mod recording {
    use super::ProgressReporter;
    use std::sync::Mutex;

    /// Test reporter that records the ordered sequence of callbacks.
    #[derive(Default)]
    pub struct Recorder {
        pub events: Mutex<Vec<String>>,
    }
    impl ProgressReporter for Recorder {
        fn on_download_start(&self, label: &str, total: Option<u64>) {
            self.events
                .lock()
                .unwrap()
                .push(format!("start:{label}:{total:?}"));
        }
        fn on_download_advance(&self, label: &str, n: u64) {
            self.events
                .lock()
                .unwrap()
                .push(format!("advance:{label}:{n}"));
        }
        fn on_download_finish(&self, label: &str) {
            self.events.lock().unwrap().push(format!("finish:{label}"));
        }
        fn on_phase(&self, phase: &str) {
            self.events.lock().unwrap().push(format!("phase:{phase}"));
        }
    }
}
