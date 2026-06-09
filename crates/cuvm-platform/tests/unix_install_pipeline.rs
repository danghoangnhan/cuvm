//! Linux install-assembler e2e on FAKE redist fixtures (no real CUDA / no network).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use cuvm_app::{Artifact, Cached, Installer};
use cuvm_core::{Arch, Os, Platform, Source, VersionMeta};
use cuvm_platform::unix::UnixInstaller;
use time::OffsetDateTime;

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

fn meta() -> VersionMeta {
    VersionMeta {
        version: "12.4.1".into(),
        source: Source::Downloaded,
        cudnn: None,
        components: vec!["cuda_nvcc".into(), "cuda_cudart".into()],
        sha256: None,
        has_lib64: true,
        installed_at: OffsetDateTime::UNIX_EPOCH,
    }
}

/// Build a merged tmp tree (post-extract shape) directly, then place it.
fn staged_tree(root: &Path) -> PathBuf {
    let tmp = root.join(".tmp-12.4.1");
    fs::create_dir_all(tmp.join("bin")).unwrap();
    fs::create_dir_all(tmp.join("lib")).unwrap();
    fs::write(tmp.join("bin/nvcc"), "#!/bin/sh\n").unwrap();
    fs::write(tmp.join("bin/nvcc.profile"), "TOP=$(_HERE_)/..\n").unwrap();
    fs::write(tmp.join("lib/libcudart.so"), "ELFPLACEHOLDER").unwrap();
    tmp
}

#[test]
fn place_creates_lib64_symlink_writes_meta_and_atomically_renames() {
    let work = tempfile::tempdir().unwrap();
    let tmp = staged_tree(work.path());
    let dst = work.path().join("versions").join("12.4.1");
    fs::create_dir_all(dst.parent().unwrap()).unwrap();

    installer().place(&tmp, &dst, &meta()).unwrap();

    // (c) atomic rename: temp tree consumed, dst exists complete.
    assert!(
        !tmp.exists(),
        "temp tree must be renamed away (never-partial)"
    );
    assert!(dst.join("bin/nvcc").is_file());

    // (a) MANDATORY lib64 -> lib symlink.
    let lib64 = dst.join("lib64");
    let link_meta = fs::symlink_metadata(&lib64).expect("lib64 must exist");
    assert!(
        link_meta.file_type().is_symlink(),
        "lib64 must be a symlink"
    );
    assert_eq!(
        fs::read_link(&lib64).unwrap(),
        Path::new("lib"),
        "relative lib64 -> lib"
    );

    // (d) relocatability: libcudart reachable through the symlink (what
    // nvcc -L$(TOP)/lib64 needs).
    assert!(
        dst.join("lib64/libcudart.so").is_file(),
        "lib64 symlink resolves libcudart"
    );

    // (b) sidecar meta round-trips.
    let written: VersionMeta =
        serde_json::from_str(&fs::read_to_string(dst.join(".cuvm-meta.json")).unwrap()).unwrap();
    assert_eq!(written, meta());
}

#[test]
fn place_is_never_partial_when_dst_parent_missing() {
    // A non-existent parent must error WITHOUT leaving a half-renamed dst.
    let work = tempfile::tempdir().unwrap();
    let tmp = staged_tree(work.path());
    let dst = work
        .path()
        .join("no")
        .join("such")
        .join("parent")
        .join("12.4.1");
    let err = installer().place(&tmp, &dst, &meta()).unwrap_err();
    assert!(!dst.exists(), "dst must not exist after a failed place");
    let _ = err; // an error is required; its text is impl-defined.
}

#[cfg(unix)]
fn write_exec(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, body).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

#[test]
fn smoke_test_errors_clearly_when_nvcc_is_absent() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("12.4.1");
    fs::create_dir_all(root.join("bin")).unwrap(); // no nvcc inside
    let err = installer().smoke_test(&root).unwrap_err();
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("nvcc") && (msg.contains("not found") || msg.contains("missing")),
        "{msg}"
    );
}

#[cfg(unix)]
#[test]
fn smoke_test_surfaces_host_gcc_breakage_with_hint() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("12.4.1");
    fs::create_dir_all(root.join("bin")).unwrap();
    // Stub nvcc that emulates an incompatible host compiler rejection.
    write_exec(
        &root.join("bin/nvcc"),
        "#!/bin/sh\n\
         echo 'unsupported GNU version! gcc versions later than 13 are not supported' 1>&2\n\
         exit 1\n",
    );
    let err = installer().smoke_test(&root).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unsupported GNU version"),
        "must surface nvcc/host-gcc stderr: {msg}"
    );
    assert!(
        msg.contains("--allow-unsupported-compiler") || msg.contains("-ccbin"),
        "must include the host-gcc hint: {msg}"
    );
}

#[cfg(unix)]
#[test]
fn smoke_test_passes_with_a_stub_nvcc_that_succeeds() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("12.4.1");
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir_all(root.join("lib")).unwrap();
    // Stub nvcc that "compiles" by writing the requested -o output and exiting 0.
    write_exec(
        &root.join("bin/nvcc"),
        "#!/bin/sh\nout=''\nwhile [ $# -gt 0 ]; do\n  if [ \"$1\" = \"-o\" ]; then shift; out=\"$1\"; fi\n  shift\ndone\nif [ -n \"$out\" ]; then : > \"$out\"; fi\nexit 0\n",
    );
    installer()
        .smoke_test(&root)
        .expect("succeeding nvcc => smoke test passes");
}

#[ignore = "requires a real nvcc + host gcc; run with CUVM_SMOKE=1"]
#[test]
fn smoke_test_real_nvcc_compile_link() {
    let root = PathBuf::from(
        std::env::var("CUVM_REAL_ROOT").expect("set CUVM_REAL_ROOT to a real toolkit"),
    );
    installer()
        .smoke_test(&root)
        .expect("real toolkit must compile+link cudart");
}
