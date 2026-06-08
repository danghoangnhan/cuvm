//! Pure mapping from a resolved `Bundle` to the OS-neutral `EnvPlan` that
//! Activators render per shell. Zero I/O (cuvm-core dependency rule).

use crate::{Bundle, EnvPlan};

/// Build the OS-neutral environment plan for an activated bundle.
///
/// The two prepend segments (`bin`, `lib64`) are exactly the breadcrumb
/// (`injected`) the Activator must strip on the next switch — see spec §2.5/§8.
pub fn plan_for(bundle: &Bundle) -> EnvPlan {
    let root = bundle.toolkit.root.to_string_lossy().into_owned();
    let bin = format!("{root}/bin");
    let lib = format!("{root}/lib64");
    EnvPlan {
        cuda_home: root.clone(),
        cuda_path: root.clone(),
        toolkit_root: root,
        prepend_path: vec![bin.clone()],
        prepend_lib: vec![lib.clone()],
        current: bundle.handle(),
        injected: vec![bin, lib],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Arch, Bundle, Os, Platform, Source, Toolkit, Version};
    use std::path::PathBuf;
    use time::OffsetDateTime;

    fn sample_bundle() -> Bundle {
        let toolkit = Toolkit {
            version: Version::parse("12.4.1").unwrap(),
            source: Source::Downloaded,
            root: PathBuf::from("/home/u/.cuvm/versions/12.4.1"),
            platform: Platform { os: Os::Linux, arch: Arch::X86_64 },
            components: vec!["cuda_nvcc".to_string(), "cuda_cudart".to_string()],
            has_lib64: false,
            installed_at: OffsetDateTime::UNIX_EPOCH,
            checksum: None,
        };
        Bundle { toolkit, cudnn: None, extra: vec![] }
    }

    #[test]
    fn plan_maps_roots_and_prepends() {
        let p = plan_for(&sample_bundle());
        assert_eq!(p.cuda_home, "/home/u/.cuvm/versions/12.4.1");
        assert_eq!(p.cuda_path, "/home/u/.cuvm/versions/12.4.1");
        assert_eq!(p.toolkit_root, "/home/u/.cuvm/versions/12.4.1");
        assert_eq!(
            p.prepend_path,
            vec!["/home/u/.cuvm/versions/12.4.1/bin".to_string()]
        );
        assert_eq!(
            p.prepend_lib,
            vec!["/home/u/.cuvm/versions/12.4.1/lib64".to_string()]
        );
    }

    #[test]
    fn plan_sets_current_and_injected_from_handle() {
        let p = plan_for(&sample_bundle());
        assert_eq!(p.current, "12.4.1");
        // injected lists exactly what the activator will prepend, in PATH-then-LIB order
        assert_eq!(
            p.injected,
            vec![
                "/home/u/.cuvm/versions/12.4.1/bin".to_string(),
                "/home/u/.cuvm/versions/12.4.1/lib64".to_string(),
            ]
        );
    }
}
