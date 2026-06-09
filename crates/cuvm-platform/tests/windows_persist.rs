//! Windows persistence: pure user-PATH rewrite + junction create/repoint state
//! machine. The pure parts run on the linux lane; the win32 syscalls are
//! `#[cfg(windows)]` (validated on the windows CI lane).

use cuvm_platform::windows::persist::compute_user_path;

#[test]
fn prepend_does_not_clobber_existing() {
    let old = r"C:\Windows;C:\Windows\System32;C:\Tools";
    let new_bin = r"C:\Users\dev\.cuvm\current\bin";
    let result = compute_user_path(old, new_bin, None);
    assert_eq!(
        result,
        r"C:\Users\dev\.cuvm\current\bin;C:\Windows;C:\Windows\System32;C:\Tools"
    );
}

#[test]
fn switching_default_strips_prior_cuvm_bin_no_dup() {
    let old = r"C:\Users\dev\.cuvm\current\bin;C:\Windows;C:\Tools";
    let new_bin = r"C:\Users\dev\.cuvm\current\bin"; // same junction path
    let prior = Some(r"C:\Users\dev\.cuvm\current\bin");
    let result = compute_user_path(old, new_bin, prior);
    assert_eq!(result, r"C:\Users\dev\.cuvm\current\bin;C:\Windows;C:\Tools");
    assert_eq!(
        result.matches(r".cuvm\current\bin").count(),
        1,
        "no duplicate cuvm bin"
    );
}

#[test]
fn long_path_is_not_truncated() {
    // > 1024 chars: proves we never go through setx's truncating path.
    let many: Vec<String> = (0..60)
        .map(|i| format!(r"C:\Program Files\App{i}\bin"))
        .collect();
    let old = many.join(";");
    assert!(old.len() > 1024);
    let new_bin = r"C:\Users\dev\.cuvm\current\bin";
    let result = compute_user_path(&old, new_bin, None);
    assert!(
        result.len() > old.len(),
        "result must contain the full old path plus new bin"
    );
    assert!(result.ends_with(&old));
}

use assert_fs::prelude::*;
use cuvm_platform::windows::junction::set_junction;

#[test]
fn junction_create_then_repoint() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child("versions/12.4/bin").create_dir_all().unwrap();
    tmp.child("versions/12.6/bin").create_dir_all().unwrap();
    let link = tmp.child("current");

    // create
    set_junction(link.path(), tmp.child("versions/12.4").path()).unwrap();
    assert!(link.path().join("bin").exists());

    // repoint (must succeed over an existing link, no manual cleanup)
    set_junction(link.path(), tmp.child("versions/12.6").path()).unwrap();
    let resolved = std::fs::canonicalize(link.path()).unwrap();
    assert!(
        resolved.ends_with("12.6"),
        "junction must now point at 12.6, got {resolved:?}"
    );
}
