//! Embedded CUDA driver-minimum compatibility table (spec §12).
//!
//! Source of truth: CUDA Toolkit Release Notes "Table 3". Encoded as data with
//! separate Linux/Windows columns. ALL comparisons use `Version`'s numeric tuple
//! `Ord` — never lexical string compares.

use crate::version::Version;
use serde::Deserialize;

/// Raw JSON shape of one row in `data/driver_ceiling.json`.
#[derive(Debug, Deserialize)]
struct RawDriverRow {
    cuda: String,
    linux_min: String,
    windows_min: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawDriverTable {
    rows: Vec<RawDriverRow>,
}

/// One parsed row: a CUDA release and its per-OS minimum driver.
/// `windows_min == None` means Windows N/A (e.g. all of CUDA 13.x).
#[derive(Debug, Clone)]
pub struct DriverRow {
    pub cuda: Version,
    pub linux_min: Version,
    pub windows_min: Option<Version>,
}

/// The full embedded driver-ceiling table.
#[derive(Debug, Clone)]
pub struct DriverCeilingTable {
    pub rows: Vec<DriverRow>,
}

/// Embedded at compile time — keeps `cuvm-core` I/O-free (spec §3).
const DRIVER_CEILING_JSON: &str = include_str!("../../data/driver_ceiling.json");

impl DriverCeilingTable {
    /// Parse the embedded table. Panics only on a corrupt embedded asset, which
    /// is a build-time bug, not a runtime condition.
    ///
    /// # Panics
    /// Panics if the embedded `data/driver_ceiling.json` is malformed. This
    /// indicates a build-time bug (corrupted asset), not a runtime condition.
    #[must_use]
    pub fn load() -> Self {
        let raw: RawDriverTable = serde_json::from_str(DRIVER_CEILING_JSON)
            .expect("embedded driver_ceiling.json is valid JSON");
        let rows = raw
            .rows
            .into_iter()
            .map(|r| DriverRow {
                cuda: Version::parse(&r.cuda)
                    .expect("embedded driver_ceiling.json: cuda field parses"),
                linux_min: Version::parse(&r.linux_min)
                    .expect("embedded driver_ceiling.json: linux_min field parses"),
                windows_min: r.windows_min.as_deref().map(|s| {
                    Version::parse(s)
                        .expect("embedded driver_ceiling.json: windows_min field parses")
                }),
            })
            .collect();
        DriverCeilingTable { rows }
    }

    /// Find the row for an exact CUDA major.minor (e.g. `12.4`).
    #[must_use]
    pub fn row_for(&self, cuda: &Version) -> Option<&DriverRow> {
        self.rows.iter().find(|r| r.cuda == *cuda)
    }
}

/// Raw JSON shape of one cuDNN matrix entry in `data/cudnn_matrix.json`.
#[derive(Debug, Deserialize)]
struct RawCudnnEntry {
    cudnn: String,
    cuda_majors: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct RawCudnnMatrix {
    entries: Vec<RawCudnnEntry>,
}

/// One parsed cuDNN line and the CUDA majors it supports (spec §12).
#[derive(Debug, Clone)]
pub struct CudnnEntry {
    pub cudnn: Version,
    pub cuda_majors: Vec<u32>,
}

/// The full embedded cuDNN ↔ CUDA matrix.
#[derive(Debug, Clone)]
pub struct CudnnMatrix {
    pub entries: Vec<CudnnEntry>,
}

const CUDNN_MATRIX_JSON: &str = include_str!("../../data/cudnn_matrix.json");

impl CudnnMatrix {
    /// Parse the embedded cuDNN matrix.
    ///
    /// # Panics
    /// Panics if the embedded `data/cudnn_matrix.json` is malformed. This
    /// indicates a build-time bug (corrupted asset), not a runtime condition.
    #[must_use]
    pub fn load() -> Self {
        let raw: RawCudnnMatrix = serde_json::from_str(CUDNN_MATRIX_JSON)
            .expect("embedded cudnn_matrix.json is valid JSON");
        let entries = raw
            .entries
            .into_iter()
            .map(|e| CudnnEntry {
                cudnn: Version::parse(&e.cudnn)
                    .expect("embedded cudnn_matrix.json: cudnn field parses"),
                cuda_majors: e.cuda_majors,
            })
            .collect();
        CudnnMatrix { entries }
    }

    /// Find the matrix entry for an exact cuDNN version (e.g. `9.23.0`).
    #[must_use]
    pub fn entry_for(&self, cudnn: &Version) -> Option<&CudnnEntry> {
        self.entries.iter().find(|e| e.cudnn == *cudnn)
    }

    /// All cuDNN line representatives whose support set includes this CUDA major.
    #[must_use]
    pub fn cudnn_lines_for_cuda_major(&self, cuda_major: u32) -> Vec<Version> {
        self.entries
            .iter()
            .filter(|e| e.cuda_majors.contains(&cuda_major))
            .map(|e| e.cudnn.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::Version;

    #[test]
    fn loads_all_driver_rows_with_no_127() {
        let t = DriverCeilingTable::load();
        // 14 GA rows from spec §12; 12.7 deliberately absent.
        assert_eq!(t.rows.len(), 14);
        assert!(
            !t.rows
                .iter()
                .any(|r| r.cuda == Version::parse("12.7").unwrap()),
            "CUDA 12.7 must not exist (NVIDIA skipped it)"
        );
    }

    #[test]
    fn driver_strings_parse_as_numeric_tuples_not_lexical() {
        let t = DriverCeilingTable::load();
        let r128 = t.row_for(&Version::parse("12.8").unwrap()).unwrap();
        let r129 = t.row_for(&Version::parse("12.9").unwrap()).unwrap();
        // 570.26 < 575.51.03 numerically; lexically "570.26" < "575..." too,
        // but 570.26 vs 570.124.06 is the real trap — assert tuple compare holds.
        assert!(r128.linux_min < r129.linux_min);
        assert!(Version::parse("570.26").unwrap() < Version::parse("570.124.06").unwrap());
    }

    #[test]
    fn windows_na_begins_at_cuda_13_0_not_13_1() {
        let t = DriverCeilingTable::load();
        // 12.9 still has a Windows minimum.
        assert!(t
            .row_for(&Version::parse("12.9").unwrap())
            .unwrap()
            .windows_min
            .is_some());
        // CRITICAL regression (spec §2.4): all of 13.x is Windows N/A, starting at 13.0.
        for v in ["13.0", "13.1", "13.2", "13.3"] {
            let row = t.row_for(&Version::parse(v).unwrap()).unwrap();
            assert!(
                row.windows_min.is_none(),
                "CUDA {v} must be Windows N/A (driver unbundled at 13.0)"
            );
        }
    }

    #[test]
    fn linux_min_is_present_for_every_row() {
        let t = DriverCeilingTable::load();
        for r in &t.rows {
            // linux_min is non-optional; this just exercises the field exists & parsed.
            assert!(r.linux_min >= Version::parse("520.0").unwrap());
        }
    }

    #[test]
    fn cudnn_matrix_loads_both_lines() {
        let m = CudnnMatrix::load();
        assert_eq!(m.entries.len(), 2);
        let last8 = m.entry_for(&Version::parse("8.9.7").unwrap()).unwrap();
        assert_eq!(last8.cuda_majors, vec![11, 12]);
        let nine = m.entry_for(&Version::parse("9.23.0").unwrap()).unwrap();
        assert_eq!(nine.cuda_majors, vec![12, 13]);
    }

    #[test]
    fn cudnn_matrix_maps_cuda_major_to_lines() {
        let m = CudnnMatrix::load();
        // CUDA 13 -> only the 9.x line supports it.
        let for13: Vec<u32> = m
            .cudnn_lines_for_cuda_major(13)
            .iter()
            .map(Version::major)
            .collect();
        assert_eq!(for13, vec![9]);
        // CUDA 11 -> only the 8.x line.
        let for11: Vec<u32> = m
            .cudnn_lines_for_cuda_major(11)
            .iter()
            .map(Version::major)
            .collect();
        assert_eq!(for11, vec![8]);
        // CUDA 12 -> both lines support it.
        let mut for12: Vec<u32> = m
            .cudnn_lines_for_cuda_major(12)
            .iter()
            .map(Version::major)
            .collect();
        for12.sort_unstable();
        assert_eq!(for12, vec![8, 9]);
    }
}
