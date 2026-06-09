//! Windows scan/adopt over a fixture tree. `scan`/`adopt` are pure filesystem
//! walks parameterized by their search roots, so they run on the linux lane.

use assert_fs::prelude::*;
use cuvm_app::Installer;
use cuvm_core::{Os, Source};
use cuvm_platform::windows::WindowsInstaller;

#[test]
fn scan_finds_and_adopts_program_files_install() {
    let tmp = assert_fs::TempDir::new().unwrap();
    // Mimic: <root>\CUDA\v12.4\bin\nvcc.exe
    tmp.child("CUDA/v12.4/bin/nvcc.exe").write_str("").unwrap();
    tmp.child("CUDA/v12.4/lib/x64/cudart.lib")
        .write_str("")
        .unwrap();
    // A non-version dir must be ignored.
    tmp.child("CUDA/extras/readme.txt").write_str("").unwrap();

    let installer = WindowsInstaller::with_roots(vec![tmp.child("CUDA").path().to_path_buf()]);

    let cands = installer.scan().unwrap();
    assert_eq!(cands.len(), 1, "expected exactly one versioned toolkit dir");
    assert_eq!(cands[0].version.raw, "12.4");

    let bundle = installer.adopt(&cands[0]).unwrap();
    assert_eq!(bundle.toolkit.source, Source::Adopted);
    assert_eq!(bundle.toolkit.version.raw, "12.4");
    assert_eq!(bundle.toolkit.root, tmp.child("CUDA/v12.4").path());
    assert_eq!(bundle.toolkit.platform.os, Os::Windows);
}

#[test]
fn scan_ignores_dir_without_nvcc() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child("CUDA/v12.4/extras/x.txt").write_str("").unwrap(); // no bin\nvcc.exe
    let installer = WindowsInstaller::with_roots(vec![tmp.child("CUDA").path().to_path_buf()]);
    assert!(installer.scan().unwrap().is_empty());
}
