//! Multi-version e2e: install TWO distinct CUDA toolkits from fake redist
//! fixtures, then prove they **coexist** and that **switching between them**
//! produces a correctly *isolated* environment per version.
//!
//! The per-command suites and the single-toolkit `lifecycle_e2e` each drive one
//! toolkit; nothing else installs two real toolkits side by side and switches.
//! The switch is the whole point of a version manager, so it gets its own
//! harness. Everything is offline (`httpmock`), no GPU, Unix-only (asserts the
//! Linux `lib64 -> lib` symlink and `:`-separated `PATH`/`LD_LIBRARY_PATH`).
#![cfg(unix)]

use assert_cmd::Command;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::Path;

// ----------------------------------------------------------------------------
// Fixture builders — redist-shaped `.tar.xz` via the system `tar` (the
// workspace ships only a pure-Rust xz *decoder*, so encoding shells out; same
// contract as `install_e2e` / `lifecycle_e2e`).
// ----------------------------------------------------------------------------

/// Build a redist toolkit component `.tar.xz`: wrapper
/// `<comp>-linux-x86_64-<comp_ver>-archive/` holding `bin/nvcc` + `<lib>`.
/// Returns `(bytes, sha256-hex)`.
fn make_component_tarxz(dir: &Path, comp: &str, comp_ver: &str, lib: &str) -> (Vec<u8>, String) {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    let wrapper = format!("{comp}-linux-x86_64-{comp_ver}-archive");
    let staging = dir.join(format!("stage-{comp}-{comp_ver}"));
    for (rel, body) in [("bin/nvcc", "#!/bin/sh\n"), (lib, "ELFPLACEHOLDER\n")] {
        let p = staging.join(&wrapper).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    let archive = dir.join(format!("{wrapper}.tar.xz"));
    let status = std::process::Command::new("tar")
        .arg("-cJf")
        .arg(&archive)
        .arg("-C")
        .arg(&staging)
        .arg(&wrapper)
        .status()
        .expect("invoke `tar -cJf` to build the fixture archive");
    assert!(status.success(), "tar must build the .tar.xz fixture");
    let bytes = std::fs::read(&archive).unwrap();
    let digest = Sha256::digest(&bytes);
    let mut sha = String::with_capacity(64);
    for b in &digest {
        write!(&mut sha, "{b:02x}").unwrap();
    }
    (bytes, sha)
}

// ----------------------------------------------------------------------------
// Mock redist server: two installable toolkits behind one index.
// ----------------------------------------------------------------------------

/// Register the manifest + both component tarballs for one toolkit `label`
/// (e.g. `12.4.1`) whose components carry `comp_ver` (e.g. `12.4.131`). The
/// `/redist/` index that lists every label is registered once by the caller.
fn serve_toolkit(server: &MockServer, fixtures: &Path, label: &str, comp_ver: &str) {
    let (nvcc_bytes, nvcc_sha) =
        make_component_tarxz(fixtures, "cuda_nvcc", comp_ver, "lib/stub.so");
    let (cudart_bytes, cudart_sha) =
        make_component_tarxz(fixtures, "cuda_cudart", comp_ver, "lib/libcudart.so");
    let nvcc_rel =
        format!("cuda_nvcc/linux-x86_64/cuda_nvcc-linux-x86_64-{comp_ver}-archive.tar.xz");
    let cudart_rel =
        format!("cuda_cudart/linux-x86_64/cuda_cudart-linux-x86_64-{comp_ver}-archive.tar.xz");
    let redistrib = format!(
        r#"{{
  "release_date": "2024-03-01",
  "cuda_nvcc": {{
    "name": "CUDA nvcc",
    "version": "{comp_ver}",
    "linux-x86_64": {{ "relative_path": "{nvcc_rel}", "sha256": "{nvcc_sha}", "size": "{nvcc_size}" }}
  }},
  "cuda_cudart": {{
    "name": "CUDA Runtime",
    "version": "{comp_ver}",
    "linux-x86_64": {{ "relative_path": "{cudart_rel}", "sha256": "{cudart_sha}", "size": "{cudart_size}" }}
  }}
}}"#,
        nvcc_size = nvcc_bytes.len(),
        cudart_size = cudart_bytes.len(),
    );
    server.mock(|when, then| {
        when.method(GET)
            .path(format!("/redist/redistrib_{label}.json"));
        then.status(200).body(redistrib);
    });
    server.mock(|when, then| {
        when.method(GET).path(format!("/redist/{nvcc_rel}"));
        then.status(200).body(nvcc_bytes);
    });
    server.mock(|when, then| {
        when.method(GET).path(format!("/redist/{cudart_rel}"));
        then.status(200).body(cudart_bytes);
    });
}

/// Stand up a redist whose index lists each `(label, comp_ver)` toolkit as
/// installable (`redistrib_<label>.json` + its two component tarballs).
fn serve_toolkits(server: &MockServer, fixtures: &Path, toolkits: &[(&str, &str)]) {
    use std::fmt::Write as _;
    let mut links = String::new();
    for (label, _) in toolkits {
        let _ = writeln!(
            links,
            "        <a href=\"redistrib_{label}.json\">redistrib_{label}.json</a>"
        );
    }
    let index = format!("<html><body>\n{links}        </body></html>");
    server.mock(|when, then| {
        when.method(GET).path("/redist/");
        then.status(200).body(index);
    });
    for (label, comp_ver) in toolkits {
        serve_toolkit(server, fixtures, label, comp_ver);
    }
}

// ----------------------------------------------------------------------------
// CLI driver helpers.
// ----------------------------------------------------------------------------

fn cuvm() -> Command {
    Command::cargo_bin("cuvm").expect("binary `cuvm` is built")
}

/// `cuvm` wired for an offline run: fake `CUVM_HOME`, the redist override, no
/// driver (nvidia-smi absent → compat gate proceeds "build-only OK"), and the
/// post-install smoke test skipped (fixtures ship a stub nvcc).
fn base(home: &TempDir, reg: &str) -> Command {
    let mut c = cuvm();
    c.env("CUVM_HOME", home.path())
        .env("CUVM_REGISTRY_URL", reg)
        .env("CUVM_NVIDIA_SMI", "/nonexistent/nvidia-smi")
        .env("CUVM_SKIP_SMOKE", "1");
    c
}

/// Absolute `versions/<ver>` root inside a `CUVM_HOME` (what activation points at).
fn root_of(home: &TempDir, ver: &str) -> String {
    home.child(format!("versions/{ver}"))
        .path()
        .display()
        .to_string()
}

/// `cuvm exec <ver>` launches a child with that toolkit active; assert the child
/// sees ITS OWN root as `CUDA_HOME`, its own `bin` at the PATH head, its own
/// `lib64`, and its own `CUVM_CURRENT`. A clean parent env makes the prepended
/// head exact.
fn assert_exec_isolates(mut cmd: Command, ver: &str, root: &str) {
    cmd.env("PATH", "/usr/bin:/bin")
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("CUVM_INJECTED")
        .args([
            "exec",
            ver,
            "--",
            "sh",
            "-c",
            "echo \"$CUDA_HOME|${PATH%%:*}|$LD_LIBRARY_PATH|$CUVM_CURRENT\"",
        ])
        .assert()
        .success()
        .stdout(contains(format!("{root}|{root}/bin|{root}/lib64|{ver}")));
}

/// No cross-version leakage: activating `to_ver` while `from_root` is the
/// *currently injected* toolkit (breadcrumb + a `from` bin already on PATH) must
/// STRIP `from`'s segments and prepend `to`'s — never stack them. This is the
/// invariant that stops `PATH`/`LD_LIBRARY_PATH` growing on every switch.
fn assert_switch_strips_previous(mut cmd: Command, to_ver: &str, from_root: &str, to_root: &str) {
    cmd.env(
        "CUVM_INJECTED",
        format!("{from_root}/bin:{from_root}/lib64"),
    )
    .env("PATH", format!("{from_root}/bin:/usr/bin:/bin"))
    .env("LD_LIBRARY_PATH", format!("{from_root}/lib64"))
    .args([
        "exec",
        to_ver,
        "--",
        "sh",
        "-c",
        "echo \"${PATH%%:*}|$PATH|$LD_LIBRARY_PATH\"",
    ])
    .assert()
    .success()
    // `to` is now at the head of PATH and LD_LIBRARY_PATH...
    .stdout(contains(format!("{to_root}/bin|")))
    .stdout(contains(format!("|{to_root}/lib64")))
    // ...and every trace of `from` is gone (no accumulation across switches).
    .stdout(contains(format!("{from_root}/bin")).not())
    .stdout(contains(format!("{from_root}/lib64")).not());
}

/// `major.minor` of a `major.minor.patch` version string (e.g. `12.4.1` → `12.4`).
fn major_minor(v: &str) -> String {
    v.split('.').take(2).collect::<Vec<_>>().join(".")
}

// ----------------------------------------------------------------------------
// The multi-version journey: install 12.4.1 + 12.6.0, prove they coexist, then
// switch between them via exec / default / per-dir pin and assert the activated
// environment is the right one — with no cross-version leakage.
// ----------------------------------------------------------------------------

#[test]
fn two_toolkits_install_coexist_and_switch_with_isolated_env() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_toolkits(
        &server,
        fixtures.path(),
        &[("12.4.1", "12.4.131"), ("12.6.0", "12.6.20")],
    );
    let reg = format!("{}/redist/", server.base_url());
    let run = || base(&home, &reg);

    let v1 = "12.4.1";
    let v2 = "12.6.0";
    let r1 = root_of(&home, v1);
    let r2 = root_of(&home, v2);

    // 1. Install both toolkits (companion libs off — this harness is about the
    //    toolkit switch, so it never touches the cuDNN/NCCL registries).
    for (spec, ver) in [("12.4", v1), ("12.6", v2)] {
        run()
            .args(["install", spec, "--no-cudnn"])
            .assert()
            .success()
            .stdout(contains(format!("cuda {ver}")));
        // Toolkit + the Linux lib64 -> lib symlink landed for each version.
        home.child(format!("versions/{ver}/bin/nvcc"))
            .assert(predicates::path::exists());
        assert!(
            std::fs::symlink_metadata(home.child(format!("versions/{ver}/lib64")).path())
                .unwrap()
                .file_type()
                .is_symlink(),
            "Linux install of {ver} must create the lib64 -> lib symlink"
        );
    }

    // 2. Both coexist: `ls` shows each, `which` resolves each to its OWN dir.
    run()
        .arg("ls")
        .assert()
        .success()
        .stdout(contains(v1).and(contains(v2)));
    run()
        .args(["which", v1])
        .assert()
        .success()
        .stdout(contains(&r1).and(contains(&r2).not()));
    run()
        .args(["which", v2])
        .assert()
        .success()
        .stdout(contains(&r2).and(contains(&r1).not()));

    // 3. Process isolation (the core proof): each version's child sees only its
    //    own toolkit env. See `assert_exec_isolates`.
    assert_exec_isolates(run(), v1, &r1);
    assert_exec_isolates(run(), v2, &r2);

    // 4. Switching the persistent `default` flips what `current` resolves to
    //    (with no breadcrumb in scope), independently for each version.
    for ver in [v1, v2] {
        run().args(["default", ver]).assert().success();
        run()
            .env_remove("CUVM_CURRENT")
            .arg("current")
            .assert()
            .success()
            .stdout(contains(ver));
    }

    // 5. Per-directory pins: two project dirs pin different versions; `current`
    //    resolves each from its own cwd via the `.cuda-version` upward walk —
    //    i.e. `cd` between projects switches CUDA without touching global state.
    let proj1 = TempDir::new().unwrap();
    let proj2 = TempDir::new().unwrap();
    for (proj, ver) in [(&proj1, v1), (&proj2, v2)] {
        run()
            .current_dir(proj.path())
            .args(["pin", ver])
            .assert()
            .success();
        run()
            .current_dir(proj.path())
            .env_remove("CUVM_CURRENT")
            .arg("current")
            .assert()
            .success()
            .stdout(contains(ver));
    }

    // 6. Switching v1 -> v2 strips v1 from PATH/LD_LIBRARY_PATH. See
    //    `assert_switch_strips_previous`.
    assert_switch_strips_previous(run(), v2, &r1, &r2);
}

// ----------------------------------------------------------------------------
// Major-version jump: an 11.x and a 12.x toolkit coexist and switch cleanly.
// Different CUDA majors have different lib layouts and (in the real world)
// different companion-lib matrices, so the cross-major switch is its own case.
// ----------------------------------------------------------------------------

#[test]
fn major_version_jump_11_and_12_coexist_and_switch() {
    let home = TempDir::new().unwrap();
    let fixtures = TempDir::new().unwrap();
    let server = MockServer::start();
    serve_toolkits(
        &server,
        fixtures.path(),
        &[("11.8.0", "11.8.89"), ("12.6.0", "12.6.20")],
    );
    let reg = format!("{}/redist/", server.base_url());
    let run = || base(&home, &reg);

    let v1 = "11.8.0";
    let v2 = "12.6.0";
    let r1 = root_of(&home, v1);
    let r2 = root_of(&home, v2);

    for (spec, ver) in [("11.8", v1), ("12.6", v2)] {
        run()
            .args(["install", spec, "--no-cudnn"])
            .assert()
            .success()
            .stdout(contains(format!("cuda {ver}")));
    }

    run()
        .arg("ls")
        .assert()
        .success()
        .stdout(contains(v1).and(contains(v2)));
    assert_exec_isolates(run(), v1, &r1);
    assert_exec_isolates(run(), v2, &r2);
    // Switch across the major boundary in both directions — neither leaks.
    assert_switch_strips_previous(run(), v2, &r1, &r2);
    assert_switch_strips_previous(run(), v1, &r2, &r1);
}

// ----------------------------------------------------------------------------
// Tier 1 — REAL two-version install + switch. Opt-in: hits the real NVIDIA
// redist, downloads two toolkits (multi-GB each) into a temp CUVM_HOME, and
// proves the switch on the REAL `nvcc`. Needs network + a host gcc/g++ (the
// install smoke test compiles+links a real `.cu`); NO GPU is required (that
// kernel is never run). Windows real runs are Tier 3, out of scope here.
//
// Run with:
//   CUVM_SMOKE=1 cargo test -p cuvm-cli --test multi_version_e2e -- --ignored
// Override the pair (two DISTINCT installable versions):
//   CUVM_SMOKE_VERSIONS="12.4.1 12.6.0" CUVM_SMOKE=1 cargo test \
//     -p cuvm-cli --test multi_version_e2e -- --ignored
// ----------------------------------------------------------------------------

#[test]
#[ignore = "real download; set CUVM_SMOKE=1 (opt: CUVM_SMOKE_VERSIONS) and run with --ignored"]
fn real_two_toolkits_install_and_switch() {
    assert_eq!(
        std::env::var("CUVM_SMOKE").as_deref(),
        Ok("1"),
        "guarded by CUVM_SMOKE=1"
    );
    let spec = std::env::var("CUVM_SMOKE_VERSIONS").unwrap_or_else(|_| "12.4.1 12.6.0".to_string());
    let vers: Vec<&str> = spec.split_whitespace().collect();
    assert_eq!(vers.len(), 2, "CUVM_SMOKE_VERSIONS must name two versions");
    assert_ne!(vers[0], vers[1], "the two versions must differ");
    let home = TempDir::new().unwrap();
    // No registry override (real NVIDIA redist), no fake nvidia-smi, and the
    // smoke test stays ON — this is the whole point of the real lane.
    let run = || {
        let mut c = cuvm();
        c.env("CUVM_HOME", home.path());
        c
    };

    // Real install of both: download -> verify sha256 -> extract -> nvcc smoke.
    // Generous per-install timeout guards against a wedged network, not slowness.
    for v in &vers {
        run()
            .args(["install", v, "--no-cudnn"])
            .timeout(std::time::Duration::from_hours(1))
            .assert()
            .success();
    }

    // Both coexist, and each version's REAL nvcc reports its own release.
    run()
        .arg("ls")
        .assert()
        .success()
        .stdout(contains(vers[0]).and(contains(vers[1])));
    for v in &vers {
        let root = root_of(&home, v);
        assert_exec_isolates(run(), v, &root);
        run()
            .args(["exec", v, "--", "nvcc", "--version"])
            .assert()
            .success()
            .stdout(contains(format!("release {}", major_minor(v))));
    }
    // The real cross-version switch strips the previous toolkit's real bins.
    let (r0, r1) = (root_of(&home, vers[0]), root_of(&home, vers[1]));
    assert_switch_strips_previous(run(), vers[1], &r0, &r1);
}
