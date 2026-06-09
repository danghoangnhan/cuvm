//! Golden snapshots of the Windows env-script emission (PowerShell + cmd) and
//! the chained prompt hook. Script emission is runtime-dispatched, so these run
//! on the gnu/linux lane. Fixture toolkit stays < 13.0 (Windows N/A from 13.0).

use cuvm_app::Activator;
use cuvm_core::{Arch, Bundle, Os, Platform, Shell, Source, Toolkit, Version};
use cuvm_platform::windows::WindowsActivator;
use time::OffsetDateTime;

fn win_bundle() -> Bundle {
    let platform = Platform {
        os: Os::Windows,
        arch: Arch::X86_64,
    };
    let toolkit = Toolkit {
        version: Version::parse("12.4.1").unwrap(),
        source: Source::Downloaded,
        root: r"C:\Users\dev\.cuvm\versions\12.4.1".into(),
        platform,
        components: vec!["cuda_nvcc".into(), "cuda_cudart".into()],
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
fn powershell_use() {
    let act = WindowsActivator::new();
    let script = act.emit_env(&win_bundle(), Shell::PowerShell).unwrap();
    insta::assert_snapshot!(script);
}

#[test]
fn powershell_strip_is_idempotent() {
    let act = WindowsActivator::new();
    let script = act.emit_env(&win_bundle(), Shell::PowerShell).unwrap();
    // The strip block references CUVM_INJECTED and -notcontains, so a repeated
    // `use` cannot duplicate the bin segment on $env:Path.
    assert!(script.contains("$env:CUVM_INJECTED -split ';'"));
    assert!(script.contains("$cuvm_inj -notcontains $_"));
}

#[test]
fn cmd_use() {
    let act = WindowsActivator::new();
    let script = act.emit_env(&win_bundle(), Shell::Cmd).unwrap();
    insta::assert_snapshot!(script);
}

#[test]
fn powershell_deactivate() {
    let act = WindowsActivator::new();
    let script = act.emit_deactivate(Shell::PowerShell).unwrap();
    insta::assert_snapshot!(script);
}

#[test]
fn powershell_hook_chains_existing_prompt() {
    let act = WindowsActivator::new();
    let script = act.hook(Shell::PowerShell).unwrap();
    // Must capture and re-invoke the prior prompt (chaining), not clobber it.
    assert!(script.contains("Get-Command prompt"));
    assert!(script.contains("cuvm"));
    insta::assert_snapshot!(script);
}

#[test]
fn cmd_hook_is_empty() {
    let act = WindowsActivator::new();
    let script = act.hook(Shell::Cmd).unwrap();
    assert_eq!(script.trim(), "");
}
