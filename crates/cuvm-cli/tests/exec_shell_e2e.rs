//! E2e tests for the M4/WU-21 activation surface: `exec`, `shell`,
//! `completions`, and the richer `ls-remote`. No GPU, no real network.

use assert_cmd::Command;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn cuvm() -> Command {
    Command::cargo_bin("cuvm").expect("binary `cuvm` is built")
}

/// A `CUVM_HOME` with one adopted toolkit (12.4.1) the Resolver can find.
fn seed_home() -> TempDir {
    let home = TempDir::new().unwrap();
    home.child("versions/12.4.1/bin").create_dir_all().unwrap();
    home.child("versions/12.4.1/lib64")
        .create_dir_all()
        .unwrap();
    home.child("manifest.json")
        .write_str(
            r#"{
"schema_version": 1,
"bundles": [
  { "version": "12.4.1", "source": "adopted",
    "path": "versions/12.4.1", "cudnn": null,
    "components": ["cuda_nvcc","cuda_cudart"], "sha256": null,
    "installed_at": "2026-06-08T00:00:00Z" }
],
"aliases": { "default": "12.4.1" },
"pins": {},
"last_driver": null
}"#,
        )
        .unwrap();
    home
}

// ---- exec ------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn exec_runs_command_with_the_toolkit_environment() {
    let home = seed_home();
    let root = home.child("versions/12.4.1");
    let expected_home = root.path().display().to_string();

    cuvm()
        .env("CUVM_HOME", home.path())
        // a deterministic parent PATH so the assertion on the prepended head is exact
        .env("PATH", "/usr/bin:/bin")
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("CUVM_INJECTED")
        .args([
            "exec",
            "12.4.1",
            "--",
            "sh",
            "-c",
            "echo \"$CUDA_HOME|${PATH%%:*}|$LD_LIBRARY_PATH|$CUVM_CURRENT\"",
        ])
        .assert()
        .success()
        .stdout(contains(format!(
            "{expected_home}|{expected_home}/bin|{expected_home}/lib64|12.4.1"
        )));
}

#[cfg(unix)]
#[test]
fn exec_resolves_a_minor_spec_to_the_installed_patch() {
    let home = seed_home();
    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["exec", "12.4", "--", "sh", "-c", "echo $CUVM_CURRENT"])
        .assert()
        .success()
        .stdout(contains("12.4.1"));
}

#[cfg(unix)]
#[test]
fn exec_propagates_the_child_exit_code() {
    let home = seed_home();
    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["exec", "12.4.1", "--", "sh", "-c", "exit 7"])
        .assert()
        .code(7);
}

#[test]
fn exec_without_a_command_is_an_error() {
    let home = seed_home();
    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["exec", "12.4.1"])
        .assert()
        .failure()
        // main.rs adds the single `cuvm: ` prefix — the message must not double it.
        .stderr(contains("cuvm: no command given").and(contains("cuvm: cuvm exec").not()));
}

#[test]
fn exec_on_an_unresolvable_spec_fails() {
    let home = seed_home();
    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["exec", "99.9", "--", "true"])
        .assert()
        .failure()
        // Pin the failure to spec RESOLUTION, not some unrelated error path.
        .stderr(contains("99.9"));
}

// ---- shell -----------------------------------------------------------------

/// Write an executable fake `$SHELL` that runs `body` (a `/bin/sh` snippet) then
/// exits 0. Returns the holding `TempDir` (keep it alive) and the script path.
#[cfg(unix)]
fn fake_shell(body: &str) -> (TempDir, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fake-shell");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\nexit 0\n")).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    (dir, path)
}

/// `shell` launches `$SHELL`; point it at a probe that records its env so the
/// test can assert the activation reached the subshell, then exits cleanly.
#[cfg(unix)]
#[test]
fn shell_launches_subshell_with_activation_applied() {
    let home = seed_home();
    let expected_home = home.child("versions/12.4.1").path().display().to_string();
    let (_probe, shell_path) =
        fake_shell("echo \"SHELL_SAW $CUDA_HOME $CUVM_CURRENT $CUVM_SHELL\"");

    cuvm()
        .env("CUVM_HOME", home.path())
        .env("SHELL", &shell_path)
        .args(["shell", "12.4.1"])
        .assert()
        .success()
        .stdout(contains(format!("SHELL_SAW {expected_home} 12.4.1 12.4.1")));
}

/// With no spec, `shell` falls back to the persistent `default` (12.4.1 here).
#[cfg(unix)]
#[test]
fn shell_with_no_spec_uses_the_default() {
    let home = seed_home();
    let (_probe, shell_path) = fake_shell("echo \"CUR=$CUVM_CURRENT\"");

    cuvm()
        .env("CUVM_HOME", home.path())
        .env("SHELL", &shell_path)
        .arg("shell")
        .assert()
        .success()
        .stdout(contains("CUR=12.4.1"));
}

/// With no spec, `shell` resolves a `.cuda-version` pin in the cwd (the branch
/// distinct from the `default` fallback above).
#[cfg(unix)]
#[test]
fn shell_with_no_spec_resolves_a_cuda_version_pin() {
    let home = seed_home();
    let (_probe, shell_path) = fake_shell("echo \"CUR=$CUVM_CURRENT\"");
    // A working dir pinned to 12.4.1 via .cuda-version.
    let work = TempDir::new().unwrap();
    work.child(".cuda-version").write_str("12.4.1\n").unwrap();

    cuvm()
        .current_dir(work.path())
        .env("CUVM_HOME", home.path())
        .env("SHELL", &shell_path)
        .arg("shell")
        .assert()
        .success()
        .stdout(contains("CUR=12.4.1"));
}

// ---- completions -----------------------------------------------------------

#[test]
fn completions_bash_emits_a_bash_completion_script() {
    cuvm()
        .args(["completions", "bash"])
        .assert()
        .success()
        // bash completions reference the binary and register a completion fn
        .stdout(contains("_cuvm").and(contains("complete")));
}

#[test]
fn completions_zsh_emits_a_compdef_header() {
    cuvm()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(contains("#compdef cuvm"));
}

#[test]
fn completions_rejects_an_unknown_shell() {
    cuvm()
        .args(["completions", "tcsh"])
        .assert()
        .failure()
        // clap rejects the value and lists the supported shells.
        .stderr(contains("invalid value").and(contains("tcsh")));
}

#[test]
fn help_lists_the_m4_commands() {
    cuvm().arg("--help").assert().success().stdout(
        contains("exec")
            .and(contains("shell"))
            .and(contains("completions")),
    );
}

// ---- richer ls-remote ------------------------------------------------------

const INDEX_HTML: &str = r#"<html><body>
<a href="redistrib_12.4.0.json">redistrib_12.4.0.json</a>
<a href="redistrib_12.4.1.json">redistrib_12.4.1.json</a>
<a href="redistrib_12.6.0.json">redistrib_12.6.0.json</a>
</body></html>"#;

#[test]
fn ls_remote_filters_by_spec_and_collapses_to_newest_patch() {
    use httpmock::prelude::*;
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let _index = server.mock(|when, then| {
        when.method(GET).path("/redist/");
        then.status(200).body(INDEX_HTML);
    });

    // `ls-remote 12.4` => only the 12.4 line collapses to its newest patch (12.4.1),
    // 12.4.0 is hidden by the collapse and 12.6.0 by the spec filter.
    cuvm()
        .env("CUVM_HOME", home.path())
        .env(
            "CUVM_REGISTRY_URL",
            format!("{}/redist/", server.base_url()),
        )
        .args(["ls-remote", "12.4"])
        .assert()
        .success()
        .stdout(contains("12.4.1"))
        .stdout(contains("12.4.0").not())
        .stdout(contains("12.6.0").not());
}

#[test]
fn ls_remote_all_versions_shows_every_patch() {
    use httpmock::prelude::*;
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
        .args(["ls-remote", "12.4", "--all-versions"])
        .assert()
        .success()
        .stdout(contains("12.4.0"))
        .stdout(contains("12.4.1"));
}

#[test]
fn ls_remote_show_urls_prints_the_full_redist_url_not_the_placeholder() {
    use httpmock::prelude::*;
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let _index = server.mock(|when, then| {
        when.method(GET).path("/redist/");
        then.status(200).body(INDEX_HTML);
    });
    let base = format!("{}/redist/", server.base_url());

    cuvm()
        .env("CUVM_HOME", home.path())
        .env("CUVM_REGISTRY_URL", &base)
        .args(["ls-remote", "12.6", "--show-urls"])
        .assert()
        .success()
        // The whole URL (base + filename), and NOT the default placeholder.
        .stdout(contains(format!("{base}redistrib_12.6.0.json")))
        .stdout(contains("<download available>").not());
}

// ---- richer ls-remote --cudnn ----------------------------------------------

const CUDNN_INDEX_HTML: &str = r#"<html><body>
<a href="redistrib_8.9.7.json">redistrib_8.9.7.json</a>
<a href="redistrib_9.7.0.json">redistrib_9.7.0.json</a>
<a href="redistrib_9.8.0.json">redistrib_9.8.0.json</a>
</body></html>"#;

#[test]
fn ls_remote_cudnn_filters_by_spec() {
    use httpmock::prelude::*;
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let _index = server.mock(|when, then| {
        when.method(GET).path("/cudnn/");
        then.status(200).body(CUDNN_INDEX_HTML);
    });

    // `ls-remote --cudnn 9.7` keeps only the 9.7 line; 9.8.0 and 8.9.7 drop out.
    cuvm()
        .env("CUVM_HOME", home.path())
        .env(
            "CUVM_CUDNN_REGISTRY_URL",
            format!("{}/cudnn/", server.base_url()),
        )
        .args(["ls-remote", "--cudnn", "9.7"])
        .assert()
        .success()
        .stdout(predicates::ord::eq("9.7.0\n"));
}

#[test]
fn ls_remote_cudnn_conflicts_with_show_urls() {
    // `--cudnn` carries no URL column, so the combination is rejected rather
    // than silently ignored.
    cuvm()
        .args(["ls-remote", "--cudnn", "--show-urls"])
        .assert()
        .failure()
        .stderr(contains("cannot be used with"));
}

// ---- ls-remote --nccl (M4 / WU-20: directory index, no manifest) -----------

const NCCL_INDEX_HTML: &str = r"<html><body>
<a href='..'>..</a>
<a href='New folder/'>New folder/</a>
<a href='v2.20.5/'>v2.20.5/</a>
<a href='v2.21.5/'>v2.21.5/</a>
<a href='v2.27.3/'>v2.27.3/</a>
</body></html>";

#[test]
fn ls_remote_nccl_lists_versions_newest_first() {
    use httpmock::prelude::*;
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let _index = server.mock(|when, then| {
        when.method(GET).path("/nccl/");
        then.status(200).body(NCCL_INDEX_HTML);
    });

    cuvm()
        .env("CUVM_HOME", home.path())
        .env(
            "CUVM_NCCL_REGISTRY_URL",
            format!("{}/nccl/", server.base_url()),
        )
        .args(["ls-remote", "--nccl"])
        .assert()
        .success()
        // newest-first, one per line, junk dirs excluded
        .stdout(predicates::ord::eq("2.27.3\n2.21.5\n2.20.5\n"));
}

#[test]
fn ls_remote_nccl_filters_by_spec() {
    use httpmock::prelude::*;
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let _index = server.mock(|when, then| {
        when.method(GET).path("/nccl/");
        then.status(200).body(NCCL_INDEX_HTML);
    });

    cuvm()
        .env("CUVM_HOME", home.path())
        .env(
            "CUVM_NCCL_REGISTRY_URL",
            format!("{}/nccl/", server.base_url()),
        )
        .args(["ls-remote", "--nccl", "2.21"])
        .assert()
        .success()
        .stdout(predicates::ord::eq("2.21.5\n"));
}

#[test]
fn ls_remote_nccl_conflicts_with_cudnn() {
    // The two product flags are mutually exclusive.
    cuvm()
        .args(["ls-remote", "--nccl", "--cudnn"])
        .assert()
        .failure()
        .stderr(contains("cannot be used with"));
}
