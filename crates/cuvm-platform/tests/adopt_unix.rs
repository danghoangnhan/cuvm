//! Integration tests for the unix adopt backend. These run on the linux/wsl CI
//! lane. They use empty files mimicking the CUDA layout — no real CUDA toolkit.
#![cfg(unix)]

use assert_fs::prelude::*;
use assert_fs::TempDir;

use cuvm_app::Installer;
use cuvm_core::domain::{Arch, Os, Platform, Source};
use cuvm_platform::unix::UnixInstaller;

fn linux() -> Platform {
    Platform {
        os: Os::Linux,
        arch: Arch::X86_64,
    }
}

/// Build a fake `/usr/local`-style tree with two valid toolkits.
/// A "valid" toolkit has bin/nvcc + bin/nvcc.profile (empty files are fine).
fn fixture_two_valid() -> TempDir {
    let root = TempDir::new().unwrap();
    for ver in ["12.4", "11.8"] {
        let base = format!("cuda-{ver}");
        root.child(format!("{base}/bin/nvcc")).touch().unwrap();
        root.child(format!("{base}/bin/nvcc.profile"))
            .touch()
            .unwrap();
        root.child(format!("{base}/lib64/libcudart.so"))
            .touch()
            .unwrap();
    }
    root
}

#[test]
fn scan_root_is_configurable_and_finds_two_valid_toolkits() {
    let root = fixture_two_valid();
    let installer = UnixInstaller::with_scan_root(root.path().to_path_buf(), linux());

    let mut found = installer.scan().expect("scan should succeed");
    found.sort_by(|a, b| a.version.cmp(&b.version));

    let versions: Vec<&str> = found.iter().map(|c| c.version.raw.as_str()).collect();
    assert_eq!(versions, vec!["11.8", "12.4"]);
    // Roots are recorded verbatim under the scan root (adopt-in-place).
    assert_eq!(found[1].root, root.path().join("cuda-12.4"));
}

#[test]
fn scan_rejects_dirs_missing_nvcc_or_profile() {
    let root = TempDir::new().unwrap();
    // Has nvcc but NOT nvcc.profile -> invalid.
    root.child("cuda-12.0/bin/nvcc").touch().unwrap();
    // Has nvcc.profile but NOT nvcc -> invalid.
    root.child("cuda-12.1/bin/nvcc.profile").touch().unwrap();
    // Empty dir matching the name pattern -> invalid.
    root.child("cuda-12.2/.keep").touch().unwrap();
    // A fully valid one to prove the scanner still returns the good entry.
    root.child("cuda-12.3/bin/nvcc").touch().unwrap();
    root.child("cuda-12.3/bin/nvcc.profile").touch().unwrap();

    let installer = UnixInstaller::with_scan_root(root.path().to_path_buf(), linux());
    let found = installer.scan().unwrap();
    let versions: Vec<&str> = found.iter().map(|c| c.version.raw.as_str()).collect();
    assert_eq!(versions, vec!["12.3"]);
}

#[test]
fn scan_resolves_cuda_symlink_target_and_dedups() {
    use std::os::unix::fs::symlink;
    let root = TempDir::new().unwrap();
    root.child("cuda-12.4/bin/nvcc").touch().unwrap();
    root.child("cuda-12.4/bin/nvcc.profile").touch().unwrap();
    // `cuda` -> `cuda-12.4` (the typical default-install symlink).
    symlink(root.path().join("cuda-12.4"), root.path().join("cuda")).unwrap();

    let installer = UnixInstaller::with_scan_root(root.path().to_path_buf(), linux());
    let found = installer.scan().unwrap();
    // The symlink resolves to the same dir as cuda-12.4, so we get exactly ONE candidate.
    assert_eq!(
        found.len(),
        1,
        "symlink target must be deduped against cuda-12.4"
    );
    assert_eq!(found[0].version.raw, "12.4");
}

#[test]
fn scan_returns_empty_when_root_missing() {
    let installer =
        UnixInstaller::with_scan_root(std::path::PathBuf::from("/nonexistent/cuvm-scan"), linux());
    assert!(installer.scan().unwrap().is_empty());
}

#[test]
fn adopt_builds_in_place_bundle_without_touching_dir() {
    let root = TempDir::new().unwrap();
    let tk = root.child("cuda-12.4");
    tk.child("bin/nvcc").touch().unwrap();
    tk.child("bin/nvcc.profile").touch().unwrap();
    tk.child("lib64/libcudart.so").touch().unwrap();

    let installer = UnixInstaller::with_scan_root(root.path().to_path_buf(), linux());
    let candidate = installer.scan().unwrap().into_iter().next().unwrap();

    // Scanned candidates carry the correct platform and source.
    assert_eq!(candidate.platform, linux());
    assert_eq!(candidate.source, Source::Adopted);

    let bundle = installer.adopt(&candidate).expect("adopt should succeed");

    assert_eq!(bundle.toolkit.version.raw, "12.4");
    assert_eq!(bundle.toolkit.source, Source::Adopted);
    // Recorded VERBATIM, in place — same path the scan found.
    assert_eq!(bundle.toolkit.root, root.path().join("cuda-12.4"));
    // Platform comes from the candidate, not current_platform().
    assert_eq!(bundle.toolkit.platform, linux());
    // Native /usr/local layout uses lib64 -> no symlink fix required.
    assert!(
        bundle.toolkit.has_lib64,
        "adopted installs are native lib64"
    );
    assert_eq!(bundle.toolkit.checksum, None);
    assert!(bundle.cudnn.is_none());
    assert!(bundle.extra.is_empty());
    assert_eq!(bundle.handle(), "12.4");

    // ADR-005: adopt must NOT mutate the external tree.
    tk.child("bin/nvcc").assert(predicates::path::is_file());
    tk.child("bin/nvcc.profile")
        .assert(predicates::path::is_file());
    tk.child("lib64/libcudart.so")
        .assert(predicates::path::is_file());
}

#[test]
fn adopt_rejects_a_root_that_is_not_a_valid_toolkit() {
    let root = TempDir::new().unwrap();
    root.child("cuda-9.9/.keep").touch().unwrap(); // no bin/nvcc

    let candidate = cuvm_core::Candidate {
        version: cuvm_core::Version::parse("9.9").unwrap(),
        root: root.path().join("cuda-9.9"),
        platform: linux(),
        source: Source::Adopted,
    };
    let installer = UnixInstaller::with_scan_root(root.path().to_path_buf(), linux());
    assert!(
        installer.adopt(&candidate).is_err(),
        "invalid root must not adopt"
    );
}

#[test]
fn adopt_path_is_relocatable_native_lib64_no_rewrite() {
    use std::fs;
    let root = TempDir::new().unwrap();
    let tk = root.child("cuda-13.0");
    tk.child("bin/nvcc").touch().unwrap();
    // nvcc.profile carries the self-locating relative TOP marker (native install).
    let profile = tk.child("bin/nvcc.profile");
    profile
        .write_str("TOP = $(_HERE_)/..\nLIBRARIES = -L$(TOP)/lib64\n")
        .unwrap();
    tk.child("lib64/libcudart.so").touch().unwrap(); // native lib64, not lib

    let installer = UnixInstaller::with_scan_root(root.path().to_path_buf(), linux());
    let c = installer.scan().unwrap().into_iter().next().unwrap();
    let before = fs::read_to_string(tk.child("bin/nvcc.profile").path()).unwrap();

    let bundle = installer.adopt(&c).unwrap();

    // No lib64->lib fix needed: native lib64 present, has_lib64 == true.
    assert!(bundle.toolkit.has_lib64);
    assert!(tk.child("lib64/libcudart.so").path().is_file());
    assert!(
        !tk.child("lib").path().exists(),
        "adopt must not create a lib symlink"
    );
    // nvcc.profile untouched — relocatability is intrinsic, adopt rewrites nothing.
    let after = fs::read_to_string(tk.child("bin/nvcc.profile").path()).unwrap();
    assert_eq!(before, after);
    assert!(after.contains("$(_HERE_)"));
}
