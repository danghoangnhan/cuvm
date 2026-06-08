use std::collections::BTreeMap;
use std::path::PathBuf;

use assert_fs::prelude::*;
use cuvm_app::{MemResolver, ResolveVia, Resolver};
use cuvm_core::{Arch, Bundle, CoreErr, Os, Platform, Source, Toolkit, Version};
use time::OffsetDateTime;

fn bundle(ver: &str) -> Bundle {
    let version = Version::parse(ver).unwrap();
    Bundle {
        toolkit: Toolkit {
            version: version.clone(),
            source: Source::Downloaded,
            root: PathBuf::from(format!("/v/{ver}")),
            platform: Platform {
                os: Os::Linux,
                arch: Arch::X86_64,
            },
            components: vec![],
            has_lib64: true,
            installed_at: OffsetDateTime::UNIX_EPOCH,
            checksum: None,
        },
        cudnn: None,
        extra: vec![],
    }
}

fn resolver(versions: &[&str], aliases: &[(&str, &str)]) -> MemResolver {
    let installed = versions.iter().map(|v| bundle(v)).collect();
    let amap: BTreeMap<String, String> = aliases
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    MemResolver::new(installed, amap)
}

#[test]
fn pin_file_resolves_with_pinfile_via() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child(".cuda-version").write_str("12.4").unwrap();
    let r = resolver(&["12.4.0", "12.4.7"], &[]);
    let got = r
        .resolve_from_dir(tmp.path())
        .unwrap()
        .expect("resolved from pin");
    assert_eq!(got.via, ResolveVia::PinFile);
    assert_eq!(got.bundle.toolkit.version.raw, "12.4.7"); // minor -> newest patch
    assert_eq!(got.spec, "12.4");
    let pin = got.pin.expect("pin attached");
    assert_eq!(pin.spec, "12.4");
}

#[test]
fn no_pin_falls_back_to_default_alias() {
    let tmp = assert_fs::TempDir::new().unwrap();
    let r = resolver(&["12.4.1", "13.0.0"], &[("default", "13.0.0")]);
    let got = r
        .resolve_from_dir(tmp.path())
        .unwrap()
        .expect("default used");
    assert_eq!(got.via, ResolveVia::Default);
    assert_eq!(got.bundle.toolkit.version.raw, "13.0.0");
}

#[test]
fn no_pin_no_default_is_none() {
    let tmp = assert_fs::TempDir::new().unwrap();
    let r = resolver(&["12.4.1"], &[]);
    assert!(r.resolve_from_dir(tmp.path()).unwrap().is_none());
}

#[test]
fn pin_to_uninstalled_version_is_not_installed() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child(".cuda-version").write_str("11.8").unwrap();
    let r = resolver(&["12.4.1"], &[]);
    let err = r.resolve_from_dir(tmp.path()).unwrap_err();
    assert_eq!(err, CoreErr::NotInstalled { spec: "11.8".into() });
}
