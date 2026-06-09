//! Windows assembler end-to-end on the Linux lane: build component `.zip`s in a
//! temp cache, run `extract_atomic` → `place`, and assert the merged versioned
//! prefix + sidecar (never-partial), plus the 13.0 empty-plan degrade guard.

use std::io::Write;
use std::path::Path;

use cuvm_app::{AcquirePlan, Artifact, Cached, Installer};
use cuvm_core::{Source, VersionMeta};
use cuvm_platform::windows::{WindowsAcquireOutcome, WindowsInstaller};
use time::OffsetDateTime;

fn zip_component(cache: &Path, zip_name: &str, wrapper: &str, files: &[(&str, &[u8])]) -> Cached {
    let path = cache.join(zip_name);
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (rel, bytes) in files {
        zip.start_file(format!("{wrapper}/{rel}"), opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    let sha = cuvm_download::sha256_file(&path).unwrap();
    Cached {
        artifact: Artifact {
            component: zip_name.trim_end_matches(".zip").into(),
            relative_path: format!("c/windows-x86_64/{zip_name}"),
            url: "https://example.test/x.zip".into(),
            sha256: sha,
            md5: None,
            size: 0,
        },
        path,
    }
}

#[test]
fn assemble_two_components_into_versioned_prefix() {
    let base = tempfile::tempdir().unwrap();
    let cache = base.path().join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let nvcc = zip_component(
        &cache,
        "cuda_nvcc.zip",
        "cuda_nvcc-windows-x86_64-12.4.131-archive",
        &[("bin/nvcc.exe", b"MZ nvcc")],
    );
    let cudart = zip_component(
        &cache,
        "cuda_cudart.zip",
        "cuda_cudart-windows-x86_64-12.4.131-archive",
        &[("lib/x64/cudart64_12.dll", b"MZ cudart")],
    );

    let dest_base = base.path().join("versions");
    let inst = WindowsInstaller::with_paths(cache, dest_base.clone(), vec![]);

    let tmp = dest_base.join(".tmp-12.4");
    let merged = inst.extract_atomic(&[nvcc, cudart], &tmp).unwrap();

    let meta = VersionMeta {
        version: "12.4.131".into(),
        source: Source::Downloaded,
        cudnn: None,
        components: vec!["cuda_nvcc".into(), "cuda_cudart".into()],
        sha256: None,
        has_lib64: false,
        installed_at: OffsetDateTime::UNIX_EPOCH,
    };
    let dst = dest_base.join("12.4");
    inst.place(&merged, &dst, &meta).unwrap();

    assert!(dst.join("bin").join("nvcc.exe").exists());
    assert!(dst.join("lib").join("x64").join("cudart64_12.dll").exists());
    assert!(!dst.join("lib64").exists(), "no lib64 on windows");
    assert!(dst.join(".cuvm-meta.json").exists());
    assert!(!tmp.exists(), "never-partial: tmp consumed");
}

#[test]
fn windows_13_0_empty_plan_degrades_to_adopt() {
    // The registry resolves CUDA >= 13.0 on windows-x86_64 to an empty artifact set
    // (Windows N/A from 13.0). The installer must degrade, never produce a partial.
    let plan = AcquirePlan {
        artifacts: vec![],
        dest_handle: "13.0".into(),
    };
    match WindowsInstaller::decide_acquire(&plan) {
        WindowsAcquireOutcome::DegradeToAdopt { reason } => {
            assert!(reason.to_lowercase().contains("13.0"), "{reason}");
        }
        other @ WindowsAcquireOutcome::Assembled(_) => {
            panic!("13.0 windows must degrade, got {other:?}")
        }
    }
}
