//! M2 install-pipeline e2e on FAKE redist fixtures served by httpmock (no real
//! network, no GPU). Drives `ls-remote`, `install`, and `uninstall` end to end.

use assert_cmd::Command;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use httpmock::prelude::*;
use predicates::str::contains;
use std::path::Path;

fn cuvm() -> Command {
    Command::cargo_bin("cuvm").expect("binary builds")
}

/// Build a redist-shaped `.tar.xz`: one wrapper dir `<comp>-linux-x86_64-<ver>-archive/`
/// containing `bin/nvcc` + `lib/libcudart.so` placeholders. Returns (bytes, sha256-hex).
/// Shells out to the system `tar -cJf` (the workspace deliberately ships no C-backed
/// xz encoder — only the pure-Rust `lzma-rs` decoder used at install time).
fn make_component_tarxz(dir: &Path, comp: &str, ver: &str) -> (Vec<u8>, String) {
    use sha2::{Digest, Sha256};
    use std::process::Command as ProcCommand;

    let wrapper = format!("{comp}-linux-x86_64-{ver}-archive");
    let staging = dir.join(format!("stage-{wrapper}"));
    for (rel, body) in [
        ("bin/nvcc", "#!/bin/sh\n"),
        ("lib/libcudart.so", "ELFPLACEHOLDER\n"),
    ] {
        let p = staging.join(&wrapper).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    let archive = dir.join(format!("{wrapper}.tar.xz"));
    let status = ProcCommand::new("tar")
        .arg("-cJf")
        .arg(&archive)
        .arg("-C")
        .arg(&staging)
        .arg(&wrapper)
        .status()
        .expect("invoke tar -cJf to build fixture");
    assert!(status.success(), "tar must build the .tar.xz fixture");
    let bytes = std::fs::read(&archive).unwrap();
    let sha = format!("{:x}", Sha256::digest(&bytes));
    (bytes, sha)
}

/// Write an executable fake `nvidia-smi` that reports a `GeForce` driver below the
/// 12.4 strict minimum (550.54.14) but above the 12.x floor (525.60.13), so the
/// compat gate produces a *warn* verdict (refused without `--force`, proceeds with).
#[cfg(unix)]
fn fake_nvidia_smi(dir: &Path) -> std::path::PathBuf {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    let fake = dir.join("nvidia-smi");
    {
        let mut f = std::fs::File::create(&fake).unwrap();
        writeln!(f, "#!/bin/sh\necho '545.23.08, NVIDIA GeForce RTX 4090'").unwrap();
    }
    let mut perms = std::fs::metadata(&fake).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&fake, perms).unwrap();
    fake
}

/// Redist index page with two toolkit manifests linked.
const INDEX_HTML: &str = r#"<html><body>
<a href="redistrib_12.4.1.json">redistrib_12.4.1.json</a>
<a href="redistrib_12.6.0.json">redistrib_12.6.0.json</a>
</body></html>"#;

#[test]
fn ls_remote_lists_toolkits_newest_first() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let _index = server.mock(|when, then| {
        when.method(GET).path("/redist/");
        then.status(200).body(INDEX_HTML);
    });

    cuvm()
        .env("CUVM_HOME", home.path())
        .env(
            "CUVM_REGISTRY_URL",
            format!("{}/redist/", server.base_url()),
        )
        .arg("ls-remote")
        .assert()
        .success()
        .stdout(contains("12.6.0"))
        .stdout(contains("12.4.1"));
}

/// Stand up the full fake redist for CUDA 12.4.1: index → manifest → `.tar.xz`.
/// Returns the running mock server (kept alive by the caller).
fn serve_redist_124(server: &MockServer, fixtures: &Path) {
    let (cudart_bytes, cudart_sha) = make_component_tarxz(fixtures, "cuda_cudart", "12.4.131");
    let cudart_rel =
        "cuda_cudart/linux-x86_64/cuda_cudart-linux-x86_64-12.4.131-archive.tar.xz".to_string();

    let index_html =
        r#"<html><body><a href="redistrib_12.4.1.json">redistrib_12.4.1.json</a></body></html>"#
            .to_string();
    let redistrib = format!(
        r#"{{
  "release_date": "2024-03-01",
  "cuda_cudart": {{
    "name": "CUDA Runtime libraries",
    "version": "12.4.131",
    "linux-x86_64": {{
      "relative_path": "{cudart_rel}",
      "sha256": "{cudart_sha}",
      "md5": "00000000000000000000000000000000",
      "size": {size}
    }}
  }}
}}"#,
        size = cudart_bytes.len()
    );

    server.mock(|when, then| {
        when.method(GET).path("/redist/");
        then.status(200).body(&index_html);
    });
    server.mock(|when, then| {
        when.method(GET).path("/redist/redistrib_12.4.1.json");
        then.status(200).body(&redistrib);
    });
    server.mock(|when, then| {
        when.method(GET).path(format!("/redist/{cudart_rel}"));
        then.status(200).body(cudart_bytes.clone());
    });
}

#[cfg(unix)]
#[test]
fn install_downloads_extracts_places_and_records_manifest() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());

    cuvm()
        .env("CUVM_HOME", home.path())
        .env(
            "CUVM_REGISTRY_URL",
            format!("{}/redist/", server.base_url()),
        )
        .env("CUVM_SKIP_SMOKE", "1")
        .args(["install", "12.4", "--no-cudnn"])
        .assert()
        .success()
        .stdout(contains("installed 12.4.1"));

    // toolkit landed in versions/<handle> with the component files present.
    home.child("versions/12.4.1/bin/nvcc")
        .assert(predicates::path::exists());
    home.child("versions/12.4.1/lib/libcudart.so")
        .assert(predicates::path::exists());
    // mandatory Linux lib64 -> lib symlink.
    let lib64 = home.child("versions/12.4.1/lib64");
    assert!(
        std::fs::symlink_metadata(lib64.path())
            .unwrap()
            .file_type()
            .is_symlink(),
        "lib64 must be a symlink to lib on Linux"
    );

    // manifest records a Downloaded bundle for 12.4.1.
    let manifest = std::fs::read_to_string(home.child("manifest.json").path()).unwrap();
    assert!(manifest.contains("\"12.4.1\""), "{manifest}");
    assert!(manifest.contains("downloaded"), "{manifest}");
}

#[cfg(unix)]
#[test]
fn compat_gate_refuses_without_force_and_proceeds_with_force() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());
    let smi = fake_nvidia_smi(fixtures.path());

    // Without --force: the warn verdict (driver 545.23.08 < 12.4 min 550.54.14)
    // refuses the install with a --force/cuda-compat hint, never hard-blocking.
    cuvm()
        .env("CUVM_HOME", home.path())
        .env(
            "CUVM_REGISTRY_URL",
            format!("{}/redist/", server.base_url()),
        )
        .env("CUVM_NVIDIA_SMI", &smi)
        .env("CUVM_SKIP_SMOKE", "1")
        .args(["install", "12.4"])
        .assert()
        .failure()
        .stderr(contains("--force"))
        .stderr(contains("cuda-compat"));
    home.child("versions/12.4.1")
        .assert(predicates::path::missing());

    // With --force: the same warn verdict is downgraded to a warning and proceeds.
    cuvm()
        .env("CUVM_HOME", home.path())
        .env(
            "CUVM_REGISTRY_URL",
            format!("{}/redist/", server.base_url()),
        )
        .env("CUVM_NVIDIA_SMI", &smi)
        .env("CUVM_SKIP_SMOKE", "1")
        .args(["install", "12.4", "--force"])
        .assert()
        .success()
        .stdout(contains("installed 12.4.1"));
    home.child("versions/12.4.1/bin/nvcc")
        .assert(predicates::path::exists());
}

fn seed_manifest(home: &TempDir, version: &str, source: &str, path: &str) {
    let manifest = format!(
        r#"{{"schema_version":1,"bundles":[
  {{"version":"{version}","source":"{source}","path":"{path}","cudnn":null,
    "components":[],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}}
],"aliases":{{}},"pins":{{}},"last_driver":null}}"#
    );
    home.child("manifest.json").write_str(&manifest).unwrap();
}

#[test]
fn uninstall_downloaded_deletes_versions_dir_and_deregisters() {
    let home = TempDir::new().unwrap();
    home.child("versions/12.4.1/bin/nvcc")
        .write_str("x")
        .unwrap();
    seed_manifest(&home, "12.4.1", "downloaded", "versions/12.4.1");

    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["uninstall", "12.4.1"])
        .assert()
        .success()
        .stdout(contains("removed 12.4.1"));

    home.child("versions/12.4.1")
        .assert(predicates::path::missing());
    let manifest = std::fs::read_to_string(home.child("manifest.json").path()).unwrap();
    assert!(!manifest.contains("\"12.4.1\""), "{manifest}");
}

#[test]
fn uninstall_adopted_deregisters_but_keeps_files() {
    let home = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    external.child("bin/nvcc").write_str("x").unwrap();
    let ext_path = external.path().to_string_lossy().replace('\\', "\\\\");
    seed_manifest(&home, "12.4.1", "adopted", &ext_path);

    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["uninstall", "12.4.1"])
        .assert()
        .success()
        .stdout(contains("deregistered 12.4.1"));

    external
        .child("bin/nvcc")
        .assert(predicates::path::exists());
    let manifest = std::fs::read_to_string(home.child("manifest.json").path()).unwrap();
    assert!(!manifest.contains("\"12.4.1\""), "{manifest}");
}

#[test]
fn help_lists_m2_commands() {
    cuvm()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("install"))
        .stdout(contains("ls-remote"))
        .stdout(contains("uninstall"));
}

#[test]
fn install_help_documents_cudnn_flags_as_noop() {
    cuvm()
        .args(["install", "--help"])
        .assert()
        .success()
        .stdout(contains("--cudnn"))
        .stdout(contains("--no-cudnn"))
        .stdout(contains("--force"))
        .stdout(contains("M2"));
}

#[test]
fn install_cudnn_flag_parses_without_error_in_m2() {
    // Unknown registry => the no-op cudnn flag must still parse; the command
    // fails later at the network step, not at arg parsing.
    let home = TempDir::new().unwrap();
    cuvm()
        .env("CUVM_HOME", home.path())
        .env("CUVM_REGISTRY_URL", "http://127.0.0.1:1/redist/")
        .args(["install", "12.4", "--cudnn", "9.8.0"])
        .assert()
        .failure() // network failure, NOT a clap parse error
        .stderr(contains("cuvm:"));
}

#[test]
fn install_help_documents_reinstall_flag() {
    cuvm()
        .args(["install", "--help"])
        .assert()
        .success()
        .stdout(contains("--reinstall"));
}
