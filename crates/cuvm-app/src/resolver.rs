//! Concrete in-memory [`Resolver`] implementation.
//!
//! [`MemResolver`] operates over a pre-built list of installed [`Bundle`]s plus
//! an alias map. It performs no I/O except in [`MemResolver::find_pin_upward`].

use std::collections::BTreeMap;
use std::path::Path;

use cuvm_core::{Bundle, CoreErr, Pin, Result, Version};

use crate::ports::{ResolveVia, Resolved, Resolver};

/// Spec resolution via an in-memory inventory (no network, minimal fs).
///
/// - `installed`: the set of installed bundles, identified by toolkit version.
/// - `aliases`: maps an alias name to another spec (possibly another alias).
pub struct MemResolver {
    installed: Vec<Bundle>,
    aliases: BTreeMap<String, String>,
}

impl MemResolver {
    /// Construct a resolver from a snapshot of the installed bundles and alias map.
    #[must_use]
    pub fn new(installed: Vec<Bundle>, aliases: BTreeMap<String, String>) -> Self {
        MemResolver { installed, aliases }
    }

    /// Installed versions sorted ascending.
    fn installed_versions(&self) -> Vec<Version> {
        let mut vs: Vec<Version> = self
            .installed
            .iter()
            .map(|b| b.toolkit.version.clone())
            .collect();
        vs.sort();
        vs
    }

    fn bundle_for(&self, v: &Version) -> Option<Bundle> {
        self.installed
            .iter()
            .find(|b| b.toolkit.version == *v)
            .cloned()
    }

    /// Return the newest installed version whose leading fields equal `prefix`.
    fn newest_with_prefix(&self, prefix: &[u32]) -> Option<Version> {
        self.installed_versions()
            .into_iter()
            .filter(|v| v.fields.len() >= prefix.len() && v.fields[..prefix.len()] == *prefix)
            .max()
    }
}

impl Resolver for MemResolver {
    fn resolve(&self, spec: &str) -> Result<Resolved> {
        // 1. Literal "latest" — global maximum.
        if spec == "latest" {
            let v = self.installed_versions().into_iter().max().ok_or_else(|| {
                CoreErr::NotInstalled {
                    spec: spec.to_string(),
                }
            })?;
            let bundle = self.bundle_for(&v).expect("version came from inventory");
            return Ok(Resolved {
                bundle,
                spec: spec.to_string(),
                via: ResolveVia::Latest,
                pin: None,
            });
        }

        // 2. Alias name (recursive, cycle-guarded) → re-resolve the target spec.
        if self.aliases.contains_key(spec) {
            let target = self.expand_alias(spec)?;
            let mut resolved = self.resolve(&target)?;
            resolved.spec = spec.to_string();
            resolved.via = ResolveVia::Alias;
            return Ok(resolved);
        }

        // 3. Version spec by field count.
        let parsed = Version::parse(spec).map_err(|_| CoreErr::NotInstalled {
            spec: spec.to_string(),
        })?;
        let prefix = parsed.fields.as_slice();
        let via = match prefix.len() {
            1 => ResolveVia::Major,
            2 => ResolveVia::Minor,
            _ => ResolveVia::Exact,
        };
        // Exact (≥3 fields): require numeric equality.
        // Minor/major: newest installed version with this prefix.
        let chosen = if via == ResolveVia::Exact {
            self.installed_versions().into_iter().find(|v| *v == parsed)
        } else {
            self.newest_with_prefix(prefix)
        };
        let v = chosen.ok_or_else(|| CoreErr::NotInstalled {
            spec: spec.to_string(),
        })?;
        let bundle = self.bundle_for(&v).expect("version came from inventory");
        Ok(Resolved {
            bundle,
            spec: spec.to_string(),
            via,
            pin: None,
        })
    }

    fn resolve_from_dir(&self, cwd: &Path) -> Result<Option<Resolved>> {
        match self.find_pin_upward(cwd)? {
            Some(pin) => {
                let mut resolved = self.resolve(&pin.spec)?;
                resolved.via = ResolveVia::PinFile;
                resolved.pin = Some(pin);
                Ok(Some(resolved))
            }
            None => {
                // No pin: fall back to the `default` alias if present.
                if self.aliases.contains_key("default") {
                    let mut resolved = self.resolve("default")?;
                    resolved.via = ResolveVia::Default;
                    Ok(Some(resolved))
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn expand_alias(&self, name: &str) -> Result<String> {
        let mut seen: Vec<String> = Vec::new();
        let mut cur = name.to_string();
        loop {
            if seen.iter().any(|s| s == &cur) {
                return Err(CoreErr::AliasCycle(name.to_string()));
            }
            match self.aliases.get(&cur) {
                Some(next) => {
                    seen.push(cur.clone());
                    cur.clone_from(next);
                }
                None => return Ok(cur), // terminal: a non-alias spec (version/latest)
            }
        }
    }

    fn find_pin_upward(&self, cwd: &Path) -> Result<Option<Pin>> {
        let mut dir: Option<&Path> = Some(cwd);
        while let Some(d) = dir {
            let candidate = d.join(".cuda-version");
            if let Ok(contents) = std::fs::read_to_string(&candidate) {
                let spec = contents.trim();
                if !spec.is_empty() {
                    return Ok(Some(Pin {
                        spec: spec.to_string(),
                        file: candidate,
                    }));
                }
                // Blank/whitespace-only file: keep walking upward.
            }
            dir = d.parent();
        }
        Ok(None)
    }
}

#[cfg(test)]
mod test_support {
    use std::path::PathBuf;

    use cuvm_core::{Arch, Bundle, Os, Platform, Source, Toolkit, Version};
    use time::OffsetDateTime;

    use super::*;

    pub fn bundle(ver: &str) -> Bundle {
        let version = Version::parse(ver).unwrap();
        let toolkit = Toolkit {
            version: version.clone(),
            source: Source::Downloaded,
            root: PathBuf::from(format!("/home/u/.cuvm/versions/{ver}")),
            platform: Platform {
                os: Os::Linux,
                arch: Arch::X86_64,
            },
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

    pub fn resolver(versions: &[&str], aliases: &[(&str, &str)]) -> MemResolver {
        let installed = versions.iter().map(|v| bundle(v)).collect();
        let amap = aliases
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        MemResolver::new(installed, amap)
    }
}

#[cfg(test)]
mod tests {
    use cuvm_core::CoreErr;

    use super::test_support::*;
    use super::*;
    use crate::ports::ResolveVia;

    // ---- Task 2.4: grammar tests -------------------------------------------

    #[test]
    fn exact_match() {
        let r = resolver(&["12.4.1", "12.4.0", "13.0.0"], &[]);
        let got = r.resolve("12.4.1").unwrap();
        assert_eq!(got.bundle.toolkit.version.raw, "12.4.1");
        assert_eq!(got.via, ResolveVia::Exact);
        assert_eq!(got.spec, "12.4.1");
    }

    #[test]
    fn minor_picks_newest_patch() {
        let r = resolver(&["12.4.0", "12.4.1", "12.4.10", "12.5.0"], &[]);
        let got = r.resolve("12.4").unwrap();
        assert_eq!(got.bundle.toolkit.version.raw, "12.4.10");
        assert_eq!(got.via, ResolveVia::Minor);
    }

    #[test]
    fn major_picks_newest_in_line_not_higher_major() {
        // `12` must select newest 12.x, NEVER 13.x.
        let r = resolver(&["12.4.1", "12.9.0", "13.0.0", "13.3.0"], &[]);
        let got = r.resolve("12").unwrap();
        assert_eq!(got.bundle.toolkit.version.raw, "12.9.0");
        assert_eq!(got.via, ResolveVia::Major);
    }

    #[test]
    fn latest_picks_global_newest() {
        let r = resolver(&["12.4.1", "13.0.0", "13.3.0"], &[]);
        let got = r.resolve("latest").unwrap();
        assert_eq!(got.bundle.toolkit.version.raw, "13.3.0");
        assert_eq!(got.via, ResolveVia::Latest);
    }

    #[test]
    fn missing_version_returns_typed_not_installed() {
        let r = resolver(&["12.4.1"], &[]);
        let err = r.resolve("11.8").unwrap_err();
        assert_eq!(
            err,
            CoreErr::NotInstalled {
                spec: "11.8".into()
            }
        );
        // message offers the install path.
        assert!(err.to_string().contains("cuvm install 11.8"));
    }

    #[test]
    fn exact_spec_not_installed_is_not_installed() {
        let r = resolver(&["12.4.1"], &[]);
        let err = r.resolve("12.4.2").unwrap_err();
        assert_eq!(
            err,
            CoreErr::NotInstalled {
                spec: "12.4.2".into()
            }
        );
    }

    // ---- Task 2.5: alias tests ---------------------------------------------

    #[test]
    fn alias_resolves_to_bundle() {
        let r = resolver(&["12.4.1"], &[("default", "12.4.1")]);
        let got = r.resolve("default").unwrap();
        assert_eq!(got.bundle.toolkit.version.raw, "12.4.1");
        assert_eq!(got.via, ResolveVia::Alias);
        assert_eq!(got.spec, "default"); // outer spec preserved as the alias name
    }

    #[test]
    fn alias_chain_resolves_recursively() {
        // ml -> stable -> 12.4 -> newest 12.4.x
        let r = resolver(
            &["12.4.0", "12.4.9"],
            &[("ml", "stable"), ("stable", "12.4")],
        );
        let got = r.resolve("ml").unwrap();
        assert_eq!(got.bundle.toolkit.version.raw, "12.4.9");
        assert_eq!(got.via, ResolveVia::Alias);
    }

    #[test]
    fn expand_alias_terminal_is_version_spec() {
        let r = resolver(&["12.4.1"], &[("default", "12.4.1")]);
        assert_eq!(r.expand_alias("default").unwrap(), "12.4.1");
    }

    #[test]
    fn alias_cycle_is_rejected() {
        // a -> b -> a
        let r = resolver(&["12.4.1"], &[("a", "b"), ("b", "a")]);
        let err = r.expand_alias("a").unwrap_err();
        assert_eq!(err, CoreErr::AliasCycle("a".into()));
        // resolve() surfaces the same typed error, not a stack overflow.
        let rerr = r.resolve("a").unwrap_err();
        assert_eq!(rerr, CoreErr::AliasCycle("a".into()));
    }

    #[test]
    fn self_referential_alias_is_cycle() {
        let r = resolver(&["12.4.1"], &[("loop", "loop")]);
        assert_eq!(
            r.expand_alias("loop").unwrap_err(),
            CoreErr::AliasCycle("loop".into())
        );
    }

    #[test]
    fn unknown_name_falls_through_to_version_parse() {
        // "12" is not an alias -> treated as a major spec.
        let r = resolver(&["12.9.0"], &[]);
        assert_eq!(r.resolve("12").unwrap().via, ResolveVia::Major);
        // a bogus non-version name with no alias -> NotInstalled.
        assert_eq!(
            r.resolve("nope").unwrap_err(),
            CoreErr::NotInstalled {
                spec: "nope".into()
            }
        );
    }
}
