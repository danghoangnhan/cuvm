//! Golden (insta) snapshots of the POSIX env-script emission. These run on the
//! gnu/Linux CI lane; the bytes are the load-bearing contract (spec §8).

use cuvm_core::{Arch, Bundle, Os, Platform, Shell, Source, Toolkit, Version};
use cuvm_platform::new_activator;
use std::path::PathBuf;
use time::OffsetDateTime;

fn bundle_1241() -> Bundle {
    let toolkit = Toolkit {
        version: Version::parse("12.4.1").unwrap(),
        source: Source::Downloaded,
        root: PathBuf::from("/home/u/.cuvm/versions/12.4.1"),
        platform: Platform {
            os: Os::Linux,
            arch: Arch::X86_64,
        },
        components: vec!["cuda_nvcc".to_string(), "cuda_cudart".to_string()],
        has_lib64: false,
        installed_at: OffsetDateTime::UNIX_EPOCH,
        checksum: None,
    };
    Bundle {
        toolkit,
        cudnn: None,
        extra: vec![],
    }
}

#[test]
fn emit_env_bash() {
    let act = new_activator(Os::Linux);
    let script = act.emit_env(&bundle_1241(), Shell::Bash).unwrap();
    insta::assert_snapshot!("emit_env_bash", script);
}

#[test]
fn emit_env_zsh() {
    let act = new_activator(Os::Linux);
    let script = act.emit_env(&bundle_1241(), Shell::Zsh).unwrap();
    insta::assert_snapshot!("emit_env_zsh", script);
}

#[test]
fn emit_deactivate_bash() {
    let act = new_activator(Os::Linux);
    let script = act.emit_deactivate(Shell::Bash).unwrap();
    insta::assert_snapshot!("emit_deactivate_bash", script);
}

#[test]
fn emit_deactivate_rejects_powershell() {
    let act = new_activator(Os::Linux);
    assert!(act.emit_deactivate(Shell::PowerShell).is_err());
}

use std::process::Command;

/// Run `script` under `bash --norc --noprofile` with the given starting PATH
/// and `LD_LIBRARY_PATH`, then echo the resulting PATH / `LD_LIBRARY_PATH` /
/// `CUVM_INJECTED` on separate lines. Returns (path, ld, injected).
fn eval_in_bash(script: &str, start_path: &str, start_ld: &str) -> (String, String, String) {
    let program = format!(
        "{script}\nprintf '%s\\n' \"$PATH\"\nprintf '%s\\n' \"$LD_LIBRARY_PATH\"\nprintf '%s\\n' \"$CUVM_INJECTED\"\n"
    );
    let out = Command::new("bash")
        .args(["--norc", "--noprofile", "-c", &program])
        .env("PATH", start_path)
        .env("LD_LIBRARY_PATH", start_ld)
        .env_remove("CUVM_INJECTED")
        .output()
        .expect("bash must be available on the gnu/Linux test lane");
    assert!(
        out.status.success(),
        "bash stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8(out.stdout).unwrap();
    let mut lines = s.lines();
    let path = lines.next().unwrap_or_default().to_string();
    let ld = lines.next().unwrap_or_default().to_string();
    let injected = lines.next().unwrap_or_default().to_string();
    (path, ld, injected)
}

#[test]
fn repeated_use_does_not_stack_path_duplicates() {
    let act = new_activator(Os::Linux);
    let script = act.emit_env(&bundle_1241(), Shell::Bash).unwrap();

    // First activation from a clean base PATH.
    let base_path = "/usr/bin:/bin";
    let base_ld = "/lib/x86_64-linux-gnu";
    let (p1, l1, inj1) = eval_in_bash(&script, base_path, base_ld);

    let bin = "/home/u/.cuvm/versions/12.4.1/bin";
    let lib = "/home/u/.cuvm/versions/12.4.1/lib64";
    assert_eq!(inj1, format!("{bin}:{lib}"));
    assert!(p1.starts_with(&format!("{bin}:")), "first PATH = {p1}");
    assert!(p1.contains("/usr/bin"), "base PATH preserved: {p1}");

    // Second activation: feed the post-1 environment back in (simulates a
    // second `use`). The breadcrumb from run 1 must be stripped first so the
    // bin/lib64 segments appear EXACTLY ONCE.
    let program = format!(
        "export CUVM_INJECTED='{inj1}'\n{script}\nprintf '%s\\n' \"$PATH\"\nprintf '%s\\n' \"$LD_LIBRARY_PATH\"\nprintf '%s\\n' \"$CUVM_INJECTED\"\n"
    );
    let out = Command::new("bash")
        .args(["--norc", "--noprofile", "-c", &program])
        .env("PATH", &p1)
        .env("LD_LIBRARY_PATH", &l1)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "bash stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8(out.stdout).unwrap();
    let mut lines = s.lines();
    let p2 = lines.next().unwrap().to_string();
    let l2 = lines.next().unwrap().to_string();

    // No duplicate stacking: the bin segment appears exactly once in PATH,
    // the lib segment exactly once in LD_LIBRARY_PATH.
    assert_eq!(p2.matches(bin).count(), 1, "PATH stacked dup: {p2}");
    assert_eq!(l2.matches(lib).count(), 1, "LD stacked dup: {l2}");
    assert!(p2.contains("/usr/bin"), "base PATH still preserved: {p2}");
}

#[test]
fn wsl_driver_path_is_never_stripped() {
    let act = new_activator(Os::Linux);
    let script = act.emit_env(&bundle_1241(), Shell::Bash).unwrap();
    let bin = "/home/u/.cuvm/versions/12.4.1/bin";
    let lib = "/home/u/.cuvm/versions/12.4.1/lib64";
    let wsl = "/usr/lib/wsl/lib";

    // Simulate a SECOND activation where the breadcrumb from a prior switch is
    // present AND /usr/lib/wsl/lib sits in LD_LIBRARY_PATH (WSL injects it).
    let program = format!(
        "export CUVM_INJECTED='{bin}:{lib}'\n{script}\nprintf '%s\\n' \"$LD_LIBRARY_PATH\"\n"
    );
    let out = Command::new("bash")
        .args(["--norc", "--noprofile", "-c", &program])
        .env("PATH", format!("{bin}:/usr/bin:/bin"))
        .env("LD_LIBRARY_PATH", format!("{lib}:{wsl}"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "bash stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ld = String::from_utf8(out.stdout).unwrap();
    let ld = ld.lines().next().unwrap();
    // WSL driver libs survive (never a breadcrumb member); lib64 appears once.
    assert!(ld.contains(wsl), "WSL driver path stripped! LD = {ld}");
    assert_eq!(ld.matches(lib).count(), 1, "lib64 stacked: {ld}");
}

#[test]
fn empty_path_edge_does_not_emit_trailing_separator() {
    let act = new_activator(Os::Linux);
    let script = act.emit_env(&bundle_1241(), Shell::Bash).unwrap();
    let bin = "/home/u/.cuvm/versions/12.4.1/bin";

    // Start with empty PATH and unset LD_LIBRARY_PATH; the breadcrumb is set so
    // the strip runs and operates on an empty PATH ($0 empty => NF==0 => dropped).
    let program = format!(
        "export CUVM_INJECTED='{bin}'\n{script}\nprintf '[%s]\\n' \"$PATH\"\nprintf '[%s]\\n' \"$LD_LIBRARY_PATH\"\n"
    );
    // Use absolute path so the empty-PATH override does not prevent bash from
    // being found (Linux execvpe resolves via the child's new PATH, not the parent's).
    let out = Command::new("/usr/bin/bash")
        .args(["--norc", "--noprofile", "-c", &program])
        .env("PATH", "")
        .env_remove("LD_LIBRARY_PATH")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "bash stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8(out.stdout).unwrap();
    let mut lines = s.lines();
    let path = lines.next().unwrap();
    let ld = lines.next().unwrap();
    // PATH = "<bin>:" + (empty stripped to "") => no trailing ":" leaks, and
    // no leading "::" appears. We assert there is no empty segment.
    assert!(!path.contains("::"), "double-separator in PATH: {path}");
    assert!(
        !path.trim_end_matches(']').ends_with(':'),
        "trailing sep: {path}"
    );
    // LD_LIBRARY_PATH started unset => result is "<lib64>" with the :+ guard
    // producing no empty tail; assert lib64 present, no double-sep, and no
    // trailing colon (which would cause the dynamic linker to search CWD).
    assert!(ld.contains("/lib64"), "lib64 missing: {ld}");
    assert!(!ld.contains("::"), "double-separator in LD: {ld}");
    assert!(
        !ld.trim_end_matches(']').ends_with(':'),
        "trailing colon in LD_LIBRARY_PATH when unset: {ld}"
    );
}

#[test]
fn breadcrumb_drift_stale_injected_strips_nothing_unexpected() {
    // Drift: CUVM_INJECTED points at segments that are NOT in PATH (stale).
    // The strip must be a no-op for real PATH entries (strip-nothing fallback)
    // and still leave a clean, deduplicated PATH after re-prepend.
    let act = new_activator(Os::Linux);
    let script = act.emit_env(&bundle_1241(), Shell::Bash).unwrap();
    let bin = "/home/u/.cuvm/versions/12.4.1/bin";
    let stale = "/home/u/.cuvm/versions/9.9.9/bin:/home/u/.cuvm/versions/9.9.9/lib64";

    let program = format!("export CUVM_INJECTED='{stale}'\n{script}\nprintf '%s\\n' \"$PATH\"\n");
    let out = Command::new("bash")
        .args(["--norc", "--noprofile", "-c", &program])
        .env("PATH", "/usr/bin:/bin:/opt/keep/bin")
        .env_remove("LD_LIBRARY_PATH")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "bash stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let path = String::from_utf8(out.stdout).unwrap();
    let path = path.lines().next().unwrap();
    // Real entries untouched (nothing matched the stale breadcrumb).
    assert!(path.contains("/opt/keep/bin"), "real entry dropped: {path}");
    assert!(path.contains("/usr/bin"), "real entry dropped: {path}");
    // The new bin is prepended exactly once.
    assert_eq!(
        path.matches(bin).count(),
        1,
        "new bin not prepended once: {path}"
    );
    // The stale 9.9.9 segments were never in PATH, so they are still absent.
    assert!(
        !path.contains("9.9.9"),
        "phantom stale segment appeared: {path}"
    );
}
