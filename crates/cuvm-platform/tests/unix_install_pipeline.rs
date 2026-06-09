//! Linux install-assembler e2e on FAKE redist fixtures (no real CUDA / no network).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use cuvm_app::{Artifact, Cached, Installer};
use cuvm_core::{Arch, Os, Platform};
use cuvm_platform::unix::UnixInstaller;

/// Build a redist-shaped `<name>.tar.xz`: files live under one wrapper dir
/// `<wrapper>/...` (exactly what NVIDIA redist tarballs ship). Returns the archive path.
fn make_redist_tarxz(
    dir: &Path,
    archive_name: &str,
    wrapper: &str,
    files: &[(&str, &str)],
) -> PathBuf {
    let staging = dir.join(format!("stage-{wrapper}"));
    for (rel, contents) in files {
        let p = staging.join(wrapper).join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, contents).unwrap();
    }
    let archive = dir.join(archive_name);
    let status = Command::new("tar")
        .arg("-cJf")
        .arg(&archive)
        .arg("-C")
        .arg(&staging)
        .arg(wrapper)
        .status()
        .expect("invoke tar -cJf to build fixture");
    assert!(status.success(), "tar must build the .tar.xz fixture");
    archive
}

fn cached(archive: PathBuf, component: &str) -> Cached {
    Cached {
        artifact: Artifact {
            component: component.into(),
            relative_path: format!(
                "{component}/linux-x86_64/{}",
                archive.file_name().unwrap().to_string_lossy()
            ),
            url: "https://example.test/x".into(),
            sha256: "00".into(),
            md5: None,
            size: 0,
        },
        path: archive,
    }
}

fn installer() -> UnixInstaller {
    UnixInstaller::new(Platform {
        os: Os::Linux,
        arch: Arch::X86_64,
    })
}

#[test]
fn extract_atomic_strips_wrapper_and_merges_components() {
    let work = tempfile::tempdir().unwrap();
    let nvcc_tar = make_redist_tarxz(
        work.path(),
        "cuda_nvcc-linux-x86_64-12.4.131-archive.tar.xz",
        "cuda_nvcc-linux-x86_64-12.4.131-archive",
        &[
            ("bin/nvcc", "#!/bin/sh\n"),
            ("bin/nvcc.profile", "TOP=$(_HERE_)/..\n"),
        ],
    );
    let cudart_tar = make_redist_tarxz(
        work.path(),
        "cuda_cudart-linux-x86_64-12.4.131-archive.tar.xz",
        "cuda_cudart-linux-x86_64-12.4.131-archive",
        &[("lib/libcudart.so", "ELFPLACEHOLDER")],
    );
    let arts = vec![
        cached(nvcc_tar, "cuda_nvcc"),
        cached(cudart_tar, "cuda_cudart"),
    ];

    let tmp = work.path().join(".tmp-12.4.1");
    let merged = installer().extract_atomic(&arts, &tmp).unwrap();

    assert_eq!(merged, tmp, "extract_atomic returns the merged prefix");
    assert!(
        merged.join("bin/nvcc").is_file(),
        "nvcc merged, wrapper stripped"
    );
    assert!(merged.join("bin/nvcc.profile").is_file());
    assert!(
        merged.join("lib/libcudart.so").is_file(),
        "cudart merged into same tree"
    );
    // The wrapper directory must NOT survive into the merged prefix.
    assert!(
        !merged
            .join("cuda_nvcc-linux-x86_64-12.4.131-archive")
            .exists(),
        "wrapper dir must be stripped"
    );
}
