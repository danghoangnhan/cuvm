//! Install-pipeline e2e on FAKE redist fixtures served by httpmock (no real
//! network, no GPU). Drives `ls-remote`, `install`, `uninstall`, and the M3
//! `cudnn install`/`cudnn ls` pairing surface end to end.

use assert_cmd::Command;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
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

    // The index lists both 12.4.1 (the installable fixture below) and 12.6.0
    // (no manifest/tarball served — present only so it scrapes as an *available*
    // download for the unified-`ls` view).
    let index_html = r#"<html><body>
        <a href="redistrib_12.4.1.json">redistrib_12.4.1.json</a>
        <a href="redistrib_12.6.0.json">redistrib_12.6.0.json</a>
        </body></html>"#
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
      "size": "{size}"
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

/// Build a redist-shaped cuDNN `.tar.xz`: wrapper
/// `cudnn-linux-x86_64-<ver>_cuda<major>-archive/` with the loader + one
/// engine sub-lib + a header (the "full set" contract needs >1 lib).
#[cfg(unix)]
fn make_cudnn_tarxz(dir: &Path, ver: &str, cuda_major: u32) -> (Vec<u8>, String) {
    use sha2::{Digest, Sha256};
    use std::process::Command as ProcCommand;

    let wrapper = format!("cudnn-linux-x86_64-{ver}_cuda{cuda_major}-archive");
    let staging = dir.join(format!("stage-{wrapper}"));
    for (rel, body) in [
        ("lib/libcudnn.so", "CUDNNPLACEHOLDER\n"),
        ("lib/libcudnn_ops.so", "CUDNNOPS\n"),
        ("include/cudnn.h", "// cudnn\n"),
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
        .expect("tar -cJf builds the cudnn fixture");
    assert!(status.success());
    let bytes = std::fs::read(&archive).unwrap();
    let sha = format!("{:x}", Sha256::digest(&bytes));
    (bytes, sha)
}

/// Mock cuDNN redist index: lists every label the fixtures know about (8.9.7
/// has no manifest served — index-only). Register this exactly ONCE per server;
/// tests that never fetch a listed label are unaffected by its presence.
#[cfg(unix)]
fn serve_cudnn_index(server: &MockServer) {
    server.mock(|when, then| {
        when.method(GET).path("/cudnn/");
        then.status(200).body(
            r#"<html><body>
            <a href="redistrib_8.9.7.json">redistrib_8.9.7.json</a>
            <a href="redistrib_9.7.0.json">redistrib_9.7.0.json</a>
            <a href="redistrib_9.8.0.json">redistrib_9.8.0.json</a>
            </body></html>"#,
        );
    });
}

/// Mock one cuDNN release: manifest at `redistrib_<label>.json` (nesting
/// linux-x86_64/cuda12) + tarball served at the verbatim `relative_path`.
/// Does NOT register the `/cudnn/` index — call [`serve_cudnn_index`] once.
/// Returns the archive's sha256.
#[cfg(unix)]
fn serve_cudnn_version(
    server: &MockServer,
    fixtures: &Path,
    label: &str,
    product_ver: &str,
) -> String {
    let (bytes, sha) = make_cudnn_tarxz(fixtures, product_ver, 12);
    let rel = format!("cudnn/linux-x86_64/cudnn-linux-x86_64-{product_ver}_cuda12-archive.tar.xz");
    let manifest = format!(
        r#"{{
  "release_label": "{label}",
  "cudnn": {{
    "license_path": "cudnn/LICENSE.txt",
    "version": "{product_ver}",
    "linux-x86_64": {{
      "cuda12": {{
        "relative_path": "{rel}",
        "sha256": "{sha}",
        "md5": "00000000000000000000000000000000",
        "size": "{size}"
      }}
    }}
  }}
}}"#,
        size = bytes.len()
    );
    server.mock(|when, then| {
        when.method(GET)
            .path(format!("/cudnn/redistrib_{label}.json"));
        then.status(200).body(manifest);
    });
    server.mock(|when, then| {
        when.method(GET).path(format!("/cudnn/{rel}"));
        then.status(200).body(bytes.clone());
    });
    sha
}

/// Index + the 9.8.0 release (the matrix-default pick for CUDA 12.x fixtures).
/// Returns the archive's sha256.
#[cfg(unix)]
fn serve_cudnn_980(server: &MockServer, fixtures: &Path) -> String {
    serve_cudnn_index(server);
    serve_cudnn_version(server, fixtures, "9.8.0", "9.8.0.87")
}

/// `cuvm` with the standard install env trio + an explicit cuDNN registry base
/// (point it at `http://127.0.0.1:1/cudnn/` to prove no cuDNN traffic happens).
#[cfg(unix)]
fn cuvm_with(home: &TempDir, registry: &str, cudnn_registry: &str) -> Command {
    let mut c = cuvm();
    c.env("CUVM_HOME", home.path())
        .env("CUVM_REGISTRY_URL", registry)
        .env("CUVM_CUDNN_REGISTRY_URL", cudnn_registry)
        .env("CUVM_SKIP_SMOKE", "1");
    c
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
        .stdout(contains("+ cuda 12.4.1")); // fresh install => the `+` marker

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
        .args(["install", "12.4", "--force", "--no-cudnn"]) // no cudnn mock here
        .assert()
        .success()
        .stdout(contains("+ cuda 12.4.1")); // fresh install => the `+` marker
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
fn install_help_documents_the_cudnn_flags() {
    cuvm()
        .args(["install", "--help"])
        .assert()
        .success()
        .stdout(
            contains("--cudnn")
                .and(contains("--no-cudnn"))
                .and(contains("--accept-eula"))
                .and(contains("--force")),
        );
}

#[test]
fn cudnn_subcommand_surfaces_in_help() {
    cuvm()
        .args(["cudnn", "--help"])
        .assert()
        .success()
        .stdout(contains("install").and(contains("ls")));
}

#[test]
fn install_help_documents_reinstall_flag() {
    cuvm()
        .args(["install", "--help"])
        .assert()
        .success()
        .stdout(contains("--reinstall"));
}

#[cfg(unix)]
#[test]
fn install_is_idempotent_and_reinstall_forces() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());

    let run = || {
        let mut c = cuvm();
        c.env("CUVM_HOME", home.path())
            .env(
                "CUVM_REGISTRY_URL",
                format!("{}/redist/", server.base_url()),
            )
            .env("CUVM_SKIP_SMOKE", "1");
        c
    };

    // First install: the fresh `+` change line on stdout (not the `~` reinstall).
    run()
        .args(["install", "12.4", "--no-cudnn"])
        .assert()
        .success()
        .stdout(contains("+ cuda 12.4.1"));

    // Second install: no-op, message on stderr, no new change line on stdout.
    run()
        .args(["install", "12.4", "--no-cudnn"])
        .assert()
        .success()
        .stderr(contains("12.4.1 is already installed"))
        .stdout(contains("cuda 12.4.1").not());

    // --reinstall: re-runs, emitting the `~` change line.
    run()
        .args(["install", "12.4", "--no-cudnn", "--reinstall"])
        .assert()
        .success()
        .stdout(contains("~ cuda 12.4.1"));
}

#[cfg(unix)]
#[test]
fn multi_install_continues_past_failure_and_exits_nonzero() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());

    // The failing spec comes FIRST: a success *after* an earlier failure is what
    // proves the loop continues instead of aborting on the first error.
    cuvm()
        .env("CUVM_HOME", home.path())
        .env(
            "CUVM_REGISTRY_URL",
            format!("{}/redist/", server.base_url()),
        )
        .env("CUVM_SKIP_SMOKE", "1")
        .args(["install", "99.9", "12.4", "--no-cudnn"])
        .assert()
        .code(1) // partial failure is exactly exit 1 (a panic's 101 must not pass)
        .stdout(contains("+ cuda 12.4.1")) // the good target still installed
        .stderr(contains("error installing 99.9"));

    // The good target really landed.
    home.child("versions/12.4.1/bin/nvcc")
        .assert(predicates::path::exists());
}

#[test]
fn ls_remote_cudnn_lists_cudnn_versions_newest_first() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let _index = server.mock(|when, then| {
        when.method(GET).path("/cudnn/");
        then.status(200).body(
            r#"<html><body>
            <a href="redistrib_8.9.7.json">redistrib_8.9.7.json</a>
            <a href="redistrib_9.8.0.json">redistrib_9.8.0.json</a>
            </body></html>"#,
        );
    });

    // Exact output: newest-first, one version per line, nothing else.
    cuvm()
        .env("CUVM_HOME", home.path())
        .env(
            "CUVM_CUDNN_REGISTRY_URL",
            format!("{}/cudnn/", server.base_url()),
        )
        .args(["ls-remote", "--cudnn"])
        .assert()
        .success()
        .stdout(predicates::ord::eq("9.8.0\n8.9.7\n"));
}

#[cfg(unix)]
#[test]
fn unified_ls_shows_installed_and_available() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());
    // Add a 12.6.0 manifest to the index so it shows as an available download.
    server.mock(|when, then| {
        when.method(GET).path("/redist/redistrib_12.6.0.json");
        then.status(200).body(r#"{"release_date":"2024-08-01"}"#);
    });

    let envs = |c: &mut Command| {
        c.env("CUVM_HOME", home.path())
            .env(
                "CUVM_REGISTRY_URL",
                format!("{}/redist/", server.base_url()),
            )
            .env("CUVM_SKIP_SMOKE", "1");
    };

    // Install warms the redist-index cache (§6.2) + lands 12.4.1, so a plain
    // `ls` (no --refresh) already sees both index entries — no cold-cache hint.
    let mut c = cuvm();
    envs(&mut c);
    c.args(["install", "12.4", "--no-cudnn"]).assert().success();

    // Unified ls (no --refresh): 12.4.1 installed (path), 12.6.0 available.
    let mut c = cuvm();
    envs(&mut c);
    c.arg("ls")
        .assert()
        .success()
        .stdout(contains("12.4.1").and(contains("versions/12.4.1")))
        .stdout(contains("12.6.0").and(contains("<download available>")));

    // --only-installed hides the available row.
    let mut c = cuvm();
    envs(&mut c);
    c.args(["ls", "--only-installed"])
        .assert()
        .success()
        .stdout(contains("12.4.1"))
        .stdout(contains("<download available>").not());

    // JSON output parses and marks installed vs available.
    let mut c = cuvm();
    envs(&mut c);
    let out = c.args(["ls", "--output-format", "json"]).assert().success();
    let json: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    let arr = json.as_array().unwrap();
    let installed_124 = arr.iter().find(|e| e["version"] == "12.4.1").unwrap();
    assert_eq!(installed_124["installed"], true, "{json}");
    // §5.5: the installed row carries the recorded source/path/components and a
    // non-null `installed_at`; `url` is reserved for available rows.
    assert_eq!(
        installed_124["components"],
        serde_json::json!(["cuda_cudart"]),
        "installed 12.4.1 must list exactly the fixture's components: {json}"
    );
    assert_eq!(installed_124["source"], "downloaded", "{json}");
    assert!(
        installed_124["path"].is_string(),
        "installed 12.4.1 must have a non-null string path: {json}"
    );
    assert!(
        installed_124["url"].is_null(),
        "installed 12.4.1 must have url: null: {json}"
    );
    assert!(
        !installed_124["installed_at"].is_null(),
        "installed 12.4.1 must have a non-null installed_at: {json}"
    );
    let avail_126 = arr.iter().find(|e| e["version"] == "12.6.0").unwrap();
    assert_eq!(avail_126["installed"], false, "{json}");
    // §5.5: available-not-installed rows point at the redist manifest and carry
    // no local state (`path`/`installed_at`/`source` all null).
    assert!(
        avail_126["url"]
            .as_str()
            .is_some_and(|u| u.ends_with("redistrib_12.6.0.json")),
        "available 12.6.0 must have a url ending in redistrib_12.6.0.json: {json}"
    );
    assert!(
        avail_126["path"].is_null(),
        "available 12.6.0 must have path: null: {json}"
    );
    assert!(
        avail_126["source"].is_null(),
        "available 12.6.0 must have source: null: {json}"
    );
    assert!(
        avail_126["installed_at"].is_null(),
        "available 12.6.0 must have installed_at: null: {json}"
    );
}

// ---- M3: cuDNN pairing (spec §7/§10, plan D5–D8) ----------------------------

#[cfg(unix)]
#[test]
fn install_pairs_cudnn_by_default_with_accepted_eula() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());
    let sha = serve_cudnn_980(&server, fixtures.path());
    let registry = format!("{}/redist/", server.base_url());
    let cudnn_reg = format!("{}/cudnn/", server.base_url());

    cuvm_with(&home, &registry, &cudnn_reg)
        .args(["install", "12.4", "--accept-eula"])
        .assert()
        .success()
        .stdout(contains("+ cuda 12.4.1"));

    // The acceptance moment was recorded once under eula/.
    home.child("eula/cudnn.json")
        .assert(predicates::path::exists());
    // The payload landed content-addressed in the store.
    home.child(format!("cudnn/{sha}/lib/libcudnn.so"))
        .assert(predicates::path::exists());
    // The full set is SYMLINKED into the toolkit (loader + sub-lib + header).
    let linked = home.child("versions/12.4.1/lib/libcudnn.so");
    assert!(
        std::fs::symlink_metadata(linked.path())
            .unwrap()
            .file_type()
            .is_symlink(),
        "libcudnn.so must be a symlink into the content store"
    );
    home.child("versions/12.4.1/lib/libcudnn_ops.so")
        .assert(predicates::path::exists());
    home.child("versions/12.4.1/include/cudnn.h")
        .assert(predicates::path::exists());
    home.child("versions/12.4.1/.cuvm-cudnn.json")
        .assert(predicates::path::exists());
    // The manifest records the PICKED index label (9.8.0), not the file version.
    let manifest = std::fs::read_to_string(home.child("manifest.json").path()).unwrap();
    assert!(manifest.contains("\"cudnn\": \"9.8.0\""), "{manifest}");
    // The VersionMeta sidecar mirrors the pairing (store_link_record tail).
    let meta: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.child("versions/12.4.1/.cuvm-meta.json").path()).unwrap(),
    )
    .unwrap();
    assert_eq!(meta["cudnn"], "9.8.0", "{meta}");
}

#[cfg(unix)]
#[test]
fn install_pairs_the_explicitly_requested_cudnn() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());
    // BOTH releases are served; the matrix default would pick 9.8.0, so a
    // broken `--cudnn` wire cannot be masked by the default pairing.
    serve_cudnn_index(&server);
    serve_cudnn_version(&server, fixtures.path(), "9.7.0", "9.7.0.66");
    serve_cudnn_version(&server, fixtures.path(), "9.8.0", "9.8.0.87");
    let registry = format!("{}/redist/", server.base_url());
    let cudnn_reg = format!("{}/cudnn/", server.base_url());

    cuvm_with(&home, &registry, &cudnn_reg)
        .args(["install", "12.4", "--cudnn", "9.7", "--accept-eula"])
        .assert()
        .success()
        .stdout(contains("+ cuda 12.4.1"));

    let manifest = std::fs::read_to_string(home.child("manifest.json").path()).unwrap();
    assert!(manifest.contains("\"cudnn\": \"9.7.0\""), "{manifest}");
    assert!(!manifest.contains("\"cudnn\": \"9.8.0\""), "{manifest}");
}

#[cfg(unix)]
#[test]
fn install_without_eula_acceptance_skips_cudnn_with_a_warning() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());
    serve_cudnn_980(&server, fixtures.path());
    let registry = format!("{}/redist/", server.base_url());
    let cudnn_reg = format!("{}/cudnn/", server.base_url());

    // Non-TTY + no --accept-eula: the toolkit install must still SUCCEED, the
    // pairing must be skipped with a notice (D7 warn-and-continue).
    cuvm_with(&home, &registry, &cudnn_reg)
        .args(["install", "12.4"])
        .assert()
        .success()
        .stdout(contains("+ cuda 12.4.1"))
        .stderr(contains("EULA"));

    home.child("eula/cudnn.json")
        .assert(predicates::path::missing());
    home.child("versions/12.4.1/.cuvm-cudnn.json")
        .assert(predicates::path::missing());
    let manifest = std::fs::read_to_string(home.child("manifest.json").path()).unwrap();
    assert!(!manifest.contains("\"cudnn\": \"9.8"), "{manifest}");
}

#[cfg(unix)]
#[test]
fn install_no_cudnn_never_touches_the_cudnn_registry() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());
    let registry = format!("{}/redist/", server.base_url());

    // The cuDNN base is UNROUTABLE: any cuDNN traffic would surface as a
    // pairing warning. --no-cudnn must produce none.
    cuvm_with(&home, &registry, "http://127.0.0.1:1/cudnn/")
        .args(["install", "12.4", "--no-cudnn"])
        .assert()
        .success()
        .stdout(contains("+ cuda 12.4.1"))
        .stderr(contains("cuDNN").not());
}

#[cfg(unix)]
#[test]
fn cudnn_install_retrofits_an_installed_toolkit() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());
    serve_cudnn_980(&server, fixtures.path());
    let registry = format!("{}/redist/", server.base_url());
    let cudnn_reg = format!("{}/cudnn/", server.base_url());

    cuvm_with(&home, &registry, &cudnn_reg)
        .args(["install", "12.4", "--no-cudnn"])
        .assert()
        .success();

    cuvm_with(&home, &registry, &cudnn_reg)
        .args([
            "cudnn",
            "install",
            "9.8",
            "--for",
            "12.4.1",
            "--accept-eula",
        ])
        .assert()
        .success()
        .stdout(contains("+ cudnn 9.8.0 (cuda12)  ->  12.4.1"));

    let linked = home.child("versions/12.4.1/lib/libcudnn.so");
    assert!(
        std::fs::symlink_metadata(linked.path())
            .unwrap()
            .file_type()
            .is_symlink(),
        "retrofit must symlink the payload into the toolkit"
    );
    home.child("versions/12.4.1/.cuvm-cudnn.json")
        .assert(predicates::path::exists());
    // Retrofit must ALSO refresh the VersionMeta sidecar (`--no-cudnn` left it
    // null); stale `cudnn: null` here was the review finding.
    let meta: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.child("versions/12.4.1/.cuvm-meta.json").path()).unwrap(),
    )
    .unwrap();
    assert_eq!(meta["cudnn"], "9.8.0", "{meta}");
}

#[cfg(unix)]
#[test]
fn cudnn_install_without_eula_on_a_pipe_is_a_hard_error() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());
    serve_cudnn_980(&server, fixtures.path());
    let registry = format!("{}/redist/", server.base_url());
    let cudnn_reg = format!("{}/cudnn/", server.base_url());

    cuvm_with(&home, &registry, &cudnn_reg)
        .args(["install", "12.4", "--no-cudnn"])
        .assert()
        .success();

    // Explicit `cudnn install` + EULA refusal = hard error (D7: it must not
    // silently no-op like the in-install default pairing does).
    cuvm_with(&home, &registry, &cudnn_reg)
        .args(["cudnn", "install", "9.8", "--for", "12.4.1"])
        .assert()
        .failure()
        .stderr(contains("EULA"));
}

#[cfg(unix)]
#[test]
fn cudnn_install_ingests_a_user_supplied_archive() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());
    let registry = format!("{}/redist/", server.base_url());

    cuvm_with(&home, &registry, "http://127.0.0.1:1/cudnn/")
        .args(["install", "12.4", "--no-cudnn"])
        .assert()
        .success();

    // Supplied archives carry their facts in the standard redist file name;
    // no EULA gate (the user already obtained the file) and no registry I/O
    // (the cuDNN base stays unroutable).
    make_cudnn_tarxz(fixtures.path(), "9.8.0.87", 12);
    let archive = fixtures
        .path()
        .join("cudnn-linux-x86_64-9.8.0.87_cuda12-archive.tar.xz");
    cuvm_with(&home, &registry, "http://127.0.0.1:1/cudnn/")
        .args([
            "cudnn",
            "install",
            archive.to_str().unwrap(),
            "--for",
            "12.4.1",
        ])
        .assert()
        .success()
        .stdout(contains("+ cudnn 9.8.0.87 (cuda12)  ->  12.4.1"));

    home.child("versions/12.4.1/lib/libcudnn_ops.so")
        .assert(predicates::path::exists());
    home.child("eula/cudnn.json")
        .assert(predicates::path::missing());
}

#[cfg(unix)]
#[test]
fn cudnn_install_refuses_an_adopted_target() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    seed_manifest(&home, "12.4", "adopted", "/usr/local/cuda-12.4");
    make_cudnn_tarxz(fixtures.path(), "9.8.0.87", 12);
    let archive = fixtures
        .path()
        .join("cudnn-linux-x86_64-9.8.0.87_cuda12-archive.tar.xz");

    cuvm()
        .env("CUVM_HOME", home.path())
        .args([
            "cudnn",
            "install",
            archive.to_str().unwrap(),
            "--for",
            "12.4",
        ])
        .assert()
        .failure()
        .stderr(contains("adopted").and(contains("never modifies")));
}

#[cfg(unix)]
#[test]
fn cudnn_install_blocks_an_incompatible_pair() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    seed_manifest(&home, "13.0.0", "downloaded", "versions/13.0.0");
    home.child("versions/13.0.0/lib").create_dir_all().unwrap();
    make_cudnn_tarxz(fixtures.path(), "8.9.7.29", 11);
    let archive = fixtures
        .path()
        .join("cudnn-linux-x86_64-8.9.7.29_cuda11-archive.tar.xz");

    cuvm()
        .env("CUVM_HOME", home.path())
        .args([
            "cudnn",
            "install",
            archive.to_str().unwrap(),
            "--for",
            "13.0.0",
        ])
        .assert()
        .failure()
        .stderr(contains("does not support CUDA 13"));
    // The block fired before any store/link mutation.
    home.child("versions/13.0.0/.cuvm-cudnn.json")
        .assert(predicates::path::missing());
}

#[cfg(unix)]
#[test]
fn cudnn_ls_shows_paired_and_unreferenced_payloads() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_redist_124(&server, fixtures.path());
    serve_cudnn_980(&server, fixtures.path());
    let registry = format!("{}/redist/", server.base_url());
    let cudnn_reg = format!("{}/cudnn/", server.base_url());

    cuvm_with(&home, &registry, &cudnn_reg)
        .args(["install", "12.4", "--accept-eula"])
        .assert()
        .success();
    // An orphaned store payload no bundle references.
    home.child("cudnn/deadbeefdeadbeef/lib")
        .create_dir_all()
        .unwrap();

    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["cudnn", "ls"])
        .assert()
        .success()
        .stdout(contains("9.8.0 (cuda12)"))
        .stdout(contains("->  12.4.1"))
        .stdout(contains("deadbeefdead  (unreferenced)"));
}
