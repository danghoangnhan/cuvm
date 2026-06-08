use std::collections::BTreeMap;

use assert_fs::prelude::*;
use cuvm_app::{MemResolver, Resolver};

fn empty_resolver() -> MemResolver {
    MemResolver::new(vec![], BTreeMap::new())
}

#[test]
fn finds_pin_in_cwd() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child(".cuda-version").write_str("12.4\n").unwrap();
    let r = empty_resolver();
    let pin = r.find_pin_upward(tmp.path()).unwrap().expect("pin found");
    assert_eq!(pin.spec, "12.4"); // trimmed
    assert_eq!(pin.file, tmp.child(".cuda-version").path());
}

#[test]
fn finds_pin_in_parent() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child(".cuda-version").write_str("13.0.0").unwrap();
    let nested = tmp.child("a/b/c");
    nested.create_dir_all().unwrap();
    let r = empty_resolver();
    let pin = r
        .find_pin_upward(nested.path())
        .unwrap()
        .expect("pin found upward");
    assert_eq!(pin.spec, "13.0.0");
    assert_eq!(pin.file, tmp.child(".cuda-version").path());
}

#[test]
fn nearest_pin_wins() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child(".cuda-version").write_str("12.0").unwrap();
    let nested = tmp.child("proj");
    nested.create_dir_all().unwrap();
    nested.child(".cuda-version").write_str("13.3").unwrap();
    let r = empty_resolver();
    let pin = r.find_pin_upward(nested.path()).unwrap().unwrap();
    assert_eq!(pin.spec, "13.3"); // nearer file shadows the ancestor
}

#[test]
fn no_pin_stops_at_fs_root() {
    // A temp dir with no .cuda-version anywhere up to the real fs root.
    let tmp = assert_fs::TempDir::new().unwrap();
    let deep = tmp.child("x/y");
    deep.create_dir_all().unwrap();
    let r = empty_resolver();
    // Must terminate (no infinite loop) and return Ok(None).
    assert!(r.find_pin_upward(deep.path()).unwrap().is_none());
}

#[test]
fn blank_pin_file_is_none_spec_trimmed() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child(".cuda-version").write_str("   \n").unwrap();
    let r = empty_resolver();
    // Whitespace-only file is treated as "no usable pin here" -> keep walking.
    assert!(r.find_pin_upward(tmp.path()).unwrap().is_none());
}
