//! First-fetch EULA acknowledgements (`$CUVM_HOME/eula/`, spec §2.3/§10):
//! cuvm relies on NVIDIA's install-and-use grant; auto-download is gated
//! behind an explicit, recorded acceptance. Never download silently.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::atomic::write_atomic;
use crate::error::Result;
use crate::layout::Layout;

const SCHEMA: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct EulaDoc {
    schema_version: u32,
    product: String,
    #[serde(with = "time::serde::rfc3339")]
    accepted_at: OffsetDateTime,
    license_url: String,
}

/// `$CUVM_HOME/eula/cudnn.json`.
#[must_use]
pub fn cudnn_eula_path(layout: &Layout) -> PathBuf {
    layout.eula_dir().join("cudnn.json")
}

/// Has the cuDNN EULA been acknowledged on this machine? The file's presence
/// alone is the record; its content is informational.
#[must_use]
pub fn cudnn_accepted(layout: &Layout) -> bool {
    cudnn_eula_path(layout).is_file()
}

/// Record the acceptance moment (re-recording overwrites the prior record;
/// clock injected — store modules never read the wall clock themselves).
///
/// # Errors
/// [`crate::StoreError::Io`] when the record cannot be written.
///
/// # Panics
/// Never in practice: serializing the in-memory acceptance record cannot fail.
pub fn record_cudnn_acceptance(
    layout: &Layout,
    now: OffsetDateTime,
    license_url: &str,
) -> Result<()> {
    let doc = EulaDoc {
        schema_version: SCHEMA,
        product: "cudnn".to_string(),
        accepted_at: now,
        license_url: license_url.to_string(),
    };
    let bytes = serde_json::to_vec_pretty(&doc).expect("EulaDoc serializes");
    write_atomic(&cudnn_eula_path(layout), &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acceptance_round_trips_and_is_idempotent() {
        let home = tempfile::tempdir().unwrap();
        let layout = Layout::new(home.path());
        assert!(!cudnn_accepted(&layout));
        let now = time::macros::datetime!(2026-06-10 10:30:00 UTC);
        record_cudnn_acceptance(&layout, now, "https://example.invalid/LICENSE.txt").unwrap();
        assert!(cudnn_accepted(&layout));
        record_cudnn_acceptance(&layout, now, "https://example.invalid/LICENSE.txt").unwrap();
        assert!(cudnn_accepted(&layout));
        let body = std::fs::read_to_string(cudnn_eula_path(&layout)).unwrap();
        assert!(body.contains("\"product\": \"cudnn\""), "{body}");
        assert!(body.contains("accepted_at"), "{body}");
    }
}
