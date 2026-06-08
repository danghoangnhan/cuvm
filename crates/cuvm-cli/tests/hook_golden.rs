//! Golden snapshot tests for `cuvm hook --shell bash|zsh`.
//!
//! On first run (or after `INSTA_UPDATE=unseen`), the snapshots are written.
//! Committed snapshots live in `tests/snapshots/`.

use assert_cmd::Command;

fn hook_stdout(shell: &str) -> String {
    let out = Command::cargo_bin("cuvm")
        .unwrap()
        .args(["hook", "--shell", shell])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "cuvm hook --shell {shell} exited non-zero; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn hook_bash() {
    insta::assert_snapshot!("hook_bash", hook_stdout("bash"));
}

#[test]
fn hook_zsh() {
    insta::assert_snapshot!("hook_zsh", hook_stdout("zsh"));
}
