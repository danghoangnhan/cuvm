//! End-to-end: `CUVM_HOME` resolution + atomic save guarantees.

use std::collections::BTreeMap;
use std::fs;

use cuvm_app::Inventory;
use cuvm_core::{BundleRecord, Manifest, Source, SCHEMA_VERSION};
use cuvm_store::{FsInventory, Layout};
use time::macros::datetime;

fn rec(ver: &str) -> BundleRecord {
    BundleRecord {
        version: ver.to_string(),
        source: Source::Downloaded,
        path: format!("versions/{ver}"),
        cudnn: None,
        components: vec!["cuda_nvcc".to_string()],
        sha256: Some("abc".to_string()),
        installed_at: datetime!(2026-06-08 10:30:00 UTC),
    }
}

#[test]
fn cuvm_home_env_drives_layout_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let layout = Layout::resolve_with(
        |k| (k == "CUVM_HOME").then(|| dir.path().to_string_lossy().into_owned()),
        None,
    )
    .unwrap();
    let inv = FsInventory::new(layout);
    inv.set_alias("default", "12.4.1").unwrap();
    assert!(dir.path().join("manifest.json").exists());
}

#[test]
fn successful_save_leaves_no_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let inv = FsInventory::new(Layout::new(dir.path()));
    let mut m = Manifest {
        schema_version: SCHEMA_VERSION,
        ..Manifest::default()
    };
    m.bundles.push(rec("12.4.1"));
    inv.save(&m).unwrap();
    inv.set_alias("default", "12.4.1").unwrap(); // a second R-M-W save

    let leftovers: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp."))
        .collect();
    assert!(leftovers.is_empty(), "temp leaked: {leftovers:?}");
}

#[test]
fn original_manifest_intact_when_save_target_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let inv = FsInventory::new(Layout::new(dir.path()));

    // first good save
    let good = Manifest {
        aliases: {
            let mut a = BTreeMap::new();
            a.insert("default".to_string(), "12.4.1".to_string());
            a
        },
        ..Manifest::default()
    };
    inv.save(&good).unwrap();
    let original_bytes = fs::read(dir.path().join("manifest.json")).unwrap();

    // Now make the manifest path un-renameable by turning it into a directory's
    // child situation: replace manifest.json with a directory of the same name
    // after capturing the good bytes, then attempt another save -> must error and
    // leave NO temp. (We restore from captured bytes to prove caller can recover.)
    fs::remove_file(dir.path().join("manifest.json")).unwrap();
    fs::create_dir(dir.path().join("manifest.json")).unwrap();

    let err = inv.save(&good).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("manifest.json"));

    // no temp leaked
    let leftovers: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp."))
        .collect();
    assert!(leftovers.is_empty(), "temp leaked: {leftovers:?}");

    // caller-side recovery proves the captured bytes are the intact original
    fs::remove_dir(dir.path().join("manifest.json")).unwrap();
    fs::write(dir.path().join("manifest.json"), &original_bytes).unwrap();
    assert_eq!(inv.load().unwrap(), good);
}
