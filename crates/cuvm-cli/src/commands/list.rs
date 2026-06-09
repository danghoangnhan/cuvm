//! `cuvm ls` — the unified installed + available view (uv `python list` shape).
//! Available rows come from the local redist-index cache (never an implicit
//! network fetch); `--only-downloads`/`--refresh` do a live fetch and refresh it.

use std::collections::BTreeMap;

use anyhow::Result;
use cuvm_app::RegistryClient;
use cuvm_core::{current_platform, Version};
use cuvm_store::{redist_cache, Layout};
use time::OffsetDateTime;

use crate::composition::{registry_base_url, Deps};

/// Cache freshness window for `ls` available rows (24h).
const CACHE_TTL_SECS: i64 = 24 * 60 * 60;

/// Options for `run_list`, built from the clap `Ls`/`LsRemote` variants.
#[derive(Debug, Default)]
#[allow(clippy::struct_excessive_bools)] // a flat mirror of the clap `Ls` flags
pub struct ListOpts {
    pub spec: Option<String>,
    pub only_installed: bool,
    pub only_downloads: bool,
    pub all_versions: bool,
    pub show_urls: bool,
    pub refresh: bool,
    pub json: bool,
}

/// One merged row in the listing.
struct Row {
    handle: String,
    version: Version,
    installed: bool,
    source: Option<&'static str>, // "downloaded" | "adopted"
    path: Option<String>,
    components: Vec<String>,
    installed_at: Option<String>, // RFC3339; `None` for available-not-installed rows
    is_default: bool,
}

/// Render the listing per `opts`. Never errors on a cold cache (installed-only +
/// a hint); `--only-downloads`/`--refresh` surface a live-fetch error.
///
/// # Errors
/// Returns an error if the manifest cannot be read, or a forced live fetch fails.
pub fn run_list(deps: &Deps, registry: &dyn RegistryClient, opts: &ListOpts) -> Result<()> {
    let layout = Layout::new(&deps.home);
    let platform = current_platform();
    let manifest = deps.inventory.load()?;
    let default = manifest.aliases.get("default").cloned();

    // Installed rows (always available, offline).
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    if !opts.only_downloads {
        for b in deps.inventory.list()? {
            let handle = b.handle();
            let source = match b.toolkit.source {
                cuvm_core::Source::Adopted => "adopted",
                _ => "downloaded",
            };
            let installed_at = b
                .toolkit
                .installed_at
                .format(&time::format_description::well_known::Rfc3339)
                .ok();
            rows.insert(
                handle.clone(),
                Row {
                    handle: handle.clone(),
                    version: b.toolkit.version.clone(),
                    installed: true,
                    source: Some(source),
                    path: Some(b.toolkit.root.display().to_string()),
                    components: b.toolkit.components.clone(),
                    installed_at,
                    is_default: default.as_deref() == Some(handle.as_str()),
                },
            );
        }
    }

    // Available rows: live fetch when forced, else the cache (no network).
    let mut cold_cache = false;
    if !opts.only_installed {
        let available = if opts.only_downloads || opts.refresh {
            let mut v = registry.list_toolkits(&platform)?;
            v.sort();
            let _ = redist_cache::write(&layout, &platform, &v, OffsetDateTime::now_utc());
            Some(v)
        } else {
            redist_cache::read(
                &layout,
                &platform,
                OffsetDateTime::now_utc(),
                CACHE_TTL_SECS,
            )
        };
        match available {
            Some(mut vers) => {
                if !opts.all_versions {
                    vers = newest_per_minor(vers);
                }
                for v in vers {
                    rows.entry(v.raw.clone()).or_insert_with(|| Row {
                        handle: v.raw.clone(),
                        version: v.clone(),
                        installed: false,
                        source: None,
                        path: None,
                        components: Vec::new(),
                        installed_at: None,
                        is_default: false,
                    });
                }
            }
            None => cold_cache = true,
        }
    }

    // Filter by spec prefix, sort newest-first.
    let mut list: Vec<Row> = rows
        .into_values()
        .filter(|r| {
            opts.spec
                .as_deref()
                .is_none_or(|s| spec_matches(s, &r.version))
        })
        .collect();
    list.sort_by(|a, b| b.version.cmp(&a.version));

    if opts.json {
        print_json(&list);
    } else {
        print_text(&list, opts.show_urls);
        if cold_cache && !opts.only_installed {
            eprintln!("(run 'cuvm ls --only-downloads' to fetch available versions)");
        }
    }
    Ok(())
}

/// Keep only the newest patch per `major.minor` (available-row collapse).
fn newest_per_minor(versions: Vec<Version>) -> Vec<Version> {
    let mut best: BTreeMap<String, Version> = BTreeMap::new();
    for v in versions {
        let key = v.major_minor().raw;
        match best.get(&key) {
            Some(cur) if *cur >= v => {}
            _ => {
                best.insert(key, v);
            }
        }
    }
    best.into_values().collect()
}

/// Whether `version` satisfies the `spec` prefix (exact / minor / major).
fn spec_matches(spec: &str, version: &Version) -> bool {
    let want: Vec<&str> = spec.split('.').collect();
    let have: Vec<String> = version.fields.iter().map(ToString::to_string).collect();
    want.len() <= have.len() && want.iter().zip(have.iter()).all(|(w, h)| w == h)
}

/// Redist manifest URL for an available version (no extra fetch).
fn redist_url(version: &Version) -> String {
    format!("{}redistrib_{}.json", registry_base_url(), version.raw)
}

fn print_text(list: &[Row], show_urls: bool) {
    if list.is_empty() {
        println!("(no toolkits)");
        return;
    }
    let width = list.iter().map(|r| r.handle.len()).max().unwrap_or(0);
    for r in list {
        let marker = if r.is_default { "*" } else { " " };
        let col2 = if r.installed {
            r.path.clone().unwrap_or_default()
        } else if show_urls {
            redist_url(&r.version)
        } else {
            "<download available>".to_string()
        };
        // `<handle> <marker>` keeps the default's "<handle> *" adjacency.
        println!("{:<width$} {marker}  {col2}", r.handle, width = width);
    }
}

fn print_json(list: &[Row]) {
    let arr: Vec<serde_json::Value> = list
        .iter()
        .map(|r| {
            serde_json::json!({
                "handle": r.handle,
                "version": r.version.raw,
                "installed": r.installed,
                "source": r.source,
                "path": r.path,
                "url": if r.installed { serde_json::Value::Null } else { redist_url(&r.version).into() },
                "default": r.is_default,
                "components": r.components,
                "installed_at": r.installed_at,
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::Value::Array(arr)).unwrap()
    );
}
