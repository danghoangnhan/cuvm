//! Filesystem-backed `Inventory` implementation.

use cuvm_app::Inventory;
use cuvm_core::{current_platform, Bundle, BundleRecord, Manifest, Source, Toolkit, Version};

use crate::layout::Layout;
use crate::manifest_io::{read_manifest, write_manifest};
use crate::meta_io::read_meta;

/// `Inventory` backed by `$CUVM_HOME/manifest.json` and per-version sidecars.
pub struct FsInventory {
    layout: Layout,
}

impl FsInventory {
    /// Create a new `FsInventory` rooted at the given layout.
    #[must_use]
    pub fn new(layout: Layout) -> Self {
        FsInventory { layout }
    }

    fn record_to_bundle(&self, rec: &BundleRecord) -> anyhow::Result<Bundle> {
        let root = self.layout.resolve_record_path(&rec.path);
        let version = Version::parse(&rec.version)?;
        // has_lib64: prefer the sidecar; adopted (no sidecar) defaults to true
        // because native /usr/local installs ship lib64 (spec §2.1).
        let has_lib64 = if rec.source == Source::Adopted {
            true
        } else {
            let meta_path = root.join(".cuvm-meta.json");
            read_meta(&meta_path).is_ok_and(|m| m.has_lib64)
        };
        let toolkit = Toolkit {
            version,
            source: rec.source,
            root,
            platform: current_platform(),
            components: rec.components.clone(),
            has_lib64,
            installed_at: rec.installed_at,
            checksum: rec.sha256.clone(),
        };
        Ok(Bundle {
            // Hydrated first: this closure borrows `toolkit.root`, and the
            // `toolkit` field below moves `toolkit` into the Bundle.
            cudnn: rec.cudnn.as_ref().and_then(|_| {
                let meta = crate::cudnn_store::read_cudnn_meta(&toolkit.root)?;
                let version = Version::parse(&meta.version).ok()?;
                Some(cuvm_core::Cudnn {
                    version,
                    cuda_major: meta.cuda_major,
                    source: meta.source,
                    store: crate::cudnn_store::store_path(&self.layout, &meta.sha256),
                    sha256: meta.sha256,
                    libs: meta.libs,
                })
            }),
            toolkit,
            extra: Vec::new(),
        })
    }
}

impl Inventory for FsInventory {
    fn list(&self) -> anyhow::Result<Vec<Bundle>> {
        let m = read_manifest(&self.layout.manifest_path())?;
        m.bundles.iter().map(|r| self.record_to_bundle(r)).collect()
    }

    fn deregister(&self, handle: &str) -> anyhow::Result<()> {
        let mut m = read_manifest(&self.layout.manifest_path())?;
        let before = m.bundles.len();
        m.bundles.retain(|b| b.version != handle);
        if m.bundles.len() == before {
            anyhow::bail!("no bundle registered with handle {handle}");
        }
        m.aliases.retain(|_, target| target != handle);
        write_manifest(&self.layout.manifest_path(), &m)?;
        Ok(())
    }

    fn set_alias(&self, n: &str, t: &str) -> anyhow::Result<()> {
        let mut m = read_manifest(&self.layout.manifest_path())?;
        m.aliases.insert(n.to_string(), t.to_string());
        write_manifest(&self.layout.manifest_path(), &m)?;
        Ok(())
    }

    fn load(&self) -> anyhow::Result<Manifest> {
        Ok(read_manifest(&self.layout.manifest_path())?)
    }

    fn save(&self, m: &Manifest) -> anyhow::Result<()> {
        write_manifest(&self.layout.manifest_path(), m)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_core::{VersionMeta, SCHEMA_VERSION};
    use std::collections::BTreeMap;
    use time::macros::datetime;

    fn inv() -> (tempfile::TempDir, FsInventory) {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        (dir, FsInventory::new(layout))
    }

    fn downloaded_record(ver: &str) -> BundleRecord {
        BundleRecord {
            version: ver.to_string(),
            source: Source::Downloaded,
            path: format!("versions/{ver}"),
            cudnn: None,
            components: vec!["cuda_nvcc".to_string()],
            sha256: Some("abc".to_string()),
            installed_at: datetime!(2026-06-08 10:30:00 UTC),
        }
    }

    fn adopted_record(ver: &str, abs: &str) -> BundleRecord {
        BundleRecord {
            version: ver.to_string(),
            source: Source::Adopted,
            path: abs.to_string(),
            cudnn: None,
            components: Vec::new(),
            sha256: None,
            installed_at: datetime!(2026-06-08 10:30:00 UTC),
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let (_d, inv) = inv();
        let mut m = Manifest::default();
        m.bundles.push(downloaded_record("12.4.1"));
        inv.save(&m).unwrap();
        assert_eq!(inv.load().unwrap(), m);
    }

    #[test]
    fn load_on_fresh_home_is_default_manifest() {
        let (_d, inv) = inv();
        assert_eq!(inv.load().unwrap(), Manifest::default());
    }

    #[test]
    fn set_alias_is_persisted() {
        let (_d, inv) = inv();
        inv.set_alias("default", "12.4.1").unwrap();
        assert_eq!(
            inv.load()
                .unwrap()
                .aliases
                .get("default")
                .map(String::as_str),
            Some("12.4.1")
        );
    }

    #[test]
    fn deregister_removes_row_and_dangling_alias() {
        let (_d, inv) = inv();
        let mut m = Manifest::default();
        m.bundles.push(downloaded_record("12.4.1"));
        m.bundles.push(downloaded_record("12.6.0"));
        let mut aliases = BTreeMap::new();
        aliases.insert("default".to_string(), "12.4.1".to_string());
        m.aliases = aliases;
        inv.save(&m).unwrap();

        inv.deregister("12.4.1").unwrap();
        let after = inv.load().unwrap();
        assert_eq!(after.bundles.len(), 1);
        assert_eq!(after.bundles[0].version, "12.6.0");
        assert!(after.aliases.is_empty(), "dangling alias not pruned");
    }

    #[test]
    fn deregister_unknown_handle_errors() {
        let (_d, inv) = inv();
        let err = inv.deregister("99.9.9").unwrap_err();
        assert!(err.to_string().contains("99.9.9"));
    }

    #[test]
    fn list_resolves_downloaded_path_under_home_and_reads_has_lib64() {
        let (dir, inv) = inv();
        let mut m = Manifest {
            schema_version: SCHEMA_VERSION,
            ..Manifest::default()
        };
        m.bundles.push(downloaded_record("12.4.1"));
        inv.save(&m).unwrap();
        // write the sidecar with has_lib64 = true (Linux post-fix state)
        let meta = VersionMeta {
            version: "12.4.1".to_string(),
            source: Source::Downloaded,
            cudnn: None,
            components: vec!["cuda_nvcc".to_string()],
            sha256: Some("abc".to_string()),
            has_lib64: true,
            installed_at: datetime!(2026-06-08 10:30:00 UTC),
        };
        crate::meta_io::write_meta(&dir.path().join("versions/12.4.1/.cuvm-meta.json"), &meta)
            .unwrap();

        let bundles = inv.list().unwrap();
        assert_eq!(bundles.len(), 1);
        let tk = &bundles[0].toolkit;
        assert_eq!(tk.root, dir.path().join("versions/12.4.1"));
        assert_eq!(tk.source, Source::Downloaded);
        assert!(tk.has_lib64);
    }

    #[test]
    fn bundles_hydrate_cudnn_from_the_sidecar() {
        let (dir, inventory) = inv();
        let mut m = Manifest::default();
        let mut rec = downloaded_record("12.4.1");
        rec.cudnn = Some("9.8.0".into());
        m.bundles.push(rec);
        inventory.save(&m).unwrap();
        // Sidecar next to the (fake) toolkit root.
        let root = dir.path().join("versions/12.4.1");
        std::fs::create_dir_all(&root).unwrap();
        crate::cudnn_store::write_cudnn_meta(
            &root,
            &cuvm_core::CudnnRecord {
                version: "9.8.0".into(),
                cuda_major: 12,
                source: Source::Downloaded,
                sha256: "feedbeef".into(),
                libs: vec!["libcudnn.so".into()],
                installed_at: datetime!(2026-06-10 10:30:00 UTC),
            },
        )
        .unwrap();

        let bundles = inventory.list().unwrap();
        let cudnn = bundles[0].cudnn.as_ref().expect("hydrated");
        assert_eq!(cudnn.version.raw, "9.8.0");
        assert_eq!(cudnn.cuda_major, 12);
        assert_eq!(cudnn.sha256, "feedbeef");
        assert_eq!(cudnn.store, dir.path().join("cudnn/feedbeef"));
        assert_eq!(cudnn.libs, vec!["libcudnn.so".to_string()]);
    }

    #[test]
    fn missing_sidecar_hydrates_as_none_not_error() {
        let (_dir, inventory) = inv();
        let mut m = Manifest::default();
        let mut rec = downloaded_record("12.4.1");
        rec.cudnn = Some("9.8.0".into());
        m.bundles.push(rec);
        inventory.save(&m).unwrap();
        let bundles = inventory.list().unwrap();
        assert!(bundles[0].cudnn.is_none());
    }

    #[test]
    fn list_keeps_adopted_absolute_path_in_place() {
        let (_d, inv) = inv();
        let mut m = Manifest::default();
        m.bundles
            .push(adopted_record("12.2.0", "/usr/local/cuda-12.2"));
        inv.save(&m).unwrap();

        let bundles = inv.list().unwrap();
        let tk = &bundles[0].toolkit;
        assert_eq!(tk.root, std::path::Path::new("/usr/local/cuda-12.2"));
        assert_eq!(tk.source, Source::Adopted);
        assert!(tk.has_lib64, "adopted native install assumed lib64");
    }
}
