//! `cuvm ls` — the unified installed + available view (uv `python list` shape).
//! Available rows come from the local redist-index cache (never an implicit
//! network fetch); `--only-downloads`/`--refresh` do a live fetch and refresh it.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
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
            Some(vers) => {
                for v in filter_and_collapse(vers, opts.spec.as_deref(), opts.all_versions) {
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
            eprintln!(
                "{}",
                crate::reporter::dim(
                    "(run 'cuvm ls --only-downloads' to fetch available versions)"
                )
            );
        }
    }
    Ok(())
}

/// `ls-remote --cudnn [<spec>]`: newest-first cuDNN versions from the cuDNN
/// redist index, optionally filtered by an exact/minor/major prefix. Live fetch
/// only — no cache (D9): the flag is explicit network intent.
///
/// # Errors
/// Returns an error if the cuDNN redist index cannot be fetched.
pub fn run_list_cudnn_remote(registry: &dyn RegistryClient, spec: Option<&str>) -> Result<()> {
    let platform = current_platform();
    let mut versions = registry
        // 0 = no CUDA-major filter — the cuDNN index is platform/major-agnostic (D4).
        .list_cudnn(&platform, 0)
        .context("fetching the cuDNN redist index")?;
    if let Some(spec) = spec {
        versions.retain(|v| spec_matches(spec, v));
    }
    versions.sort();
    for v in versions.iter().rev() {
        println!("{}", v.raw);
    }
    Ok(())
}

/// Apply the `spec` filter, then (unless `all_versions`) collapse to the
/// newest patch per minor. Filtering FIRST keeps an exact-patch query for an
/// older patch visible — collapsing first would drop it before it can match.
fn filter_and_collapse(
    mut versions: Vec<Version>,
    spec: Option<&str>,
    all_versions: bool,
) -> Vec<Version> {
    if let Some(spec) = spec {
        versions.retain(|v| spec_matches(spec, v));
    }
    if all_versions {
        versions
    } else {
        newest_per_minor(versions)
    }
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
    redist_url_for(&registry_base_url(), version)
}

/// Pure core of [`redist_url`]: join `base` and the manifest filename,
/// enforcing a trailing `/` on `base` the same way the registry client does.
fn redist_url_for(base: &str, version: &Version) -> String {
    let sep = if base.ends_with('/') { "" } else { "/" };
    format!("{base}{sep}redistrib_{}.json", version.raw)
}

fn print_text(list: &[Row], show_urls: bool) {
    if list.is_empty() {
        println!("(no toolkits)");
        return;
    }
    for line in format_text_rows(list, show_urls) {
        println!("{line}");
    }
}

/// Render the non-empty text rows, one string per row. The default's `*` is
/// glued to its handle FIRST (`<handle> *`, spec §5.5), then the composed
/// cell is padded so column 2 stays aligned across mixed-width handles.
fn format_text_rows(list: &[Row], show_urls: bool) -> Vec<String> {
    let cells: Vec<String> = list
        .iter()
        .map(|r| {
            if r.is_default {
                format!("{} *", r.handle)
            } else {
                r.handle.clone()
            }
        })
        .collect();
    let width = cells.iter().map(String::len).max().unwrap_or(0);
    list.iter()
        .zip(cells)
        .map(|(r, cell)| {
            let col2 = if r.installed {
                r.path.clone().unwrap_or_default()
            } else if show_urls {
                redist_url(&r.version)
            } else {
                "<download available>".to_string()
            };
            format!("{cell:<width$}  {col2}")
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).expect("test version parses")
    }

    fn raws(versions: &[Version]) -> Vec<&str> {
        versions.iter().map(|x| x.raw.as_str()).collect()
    }

    fn row(handle: &str, is_default: bool, installed: bool, path: Option<&str>) -> Row {
        Row {
            handle: handle.to_string(),
            version: v(handle),
            installed,
            source: installed.then_some("downloaded"),
            path: path.map(ToString::to_string),
            components: Vec::new(),
            installed_at: None,
            is_default,
        }
    }

    #[test]
    fn exact_patch_spec_survives_the_newest_per_minor_collapse() {
        // 12.4.0 is a real downloadable version; the collapse must not hide it
        // from an exact-patch query (`cuvm install 12.4.0` would succeed).
        let got = filter_and_collapse(vec![v("12.4.0"), v("12.4.1")], Some("12.4.0"), false);
        assert_eq!(raws(&got), ["12.4.0"]);
    }

    #[test]
    fn minor_spec_still_collapses_to_the_newest_patch() {
        let got = filter_and_collapse(
            vec![v("12.4.0"), v("12.4.1"), v("12.6.0")],
            Some("12.4"),
            false,
        );
        assert_eq!(raws(&got), ["12.4.1"]);
    }

    #[test]
    fn all_versions_keeps_every_matching_patch() {
        let got = filter_and_collapse(
            vec![v("12.4.0"), v("12.4.1"), v("12.6.0")],
            Some("12.4"),
            true,
        );
        assert_eq!(raws(&got), ["12.4.0", "12.4.1"]);
    }

    #[test]
    fn redist_url_for_normalizes_a_missing_trailing_slash() {
        let ver = v("12.4.1");
        assert_eq!(
            redist_url_for("http://host/redist", &ver),
            "http://host/redist/redistrib_12.4.1.json"
        );
        assert_eq!(
            redist_url_for("http://host/redist/", &ver),
            "http://host/redist/redistrib_12.4.1.json"
        );
    }

    #[test]
    fn default_marker_stays_glued_with_mixed_width_handles() {
        // The default handle is SHORTER than the longest handle; the marker
        // must stay glued (`12.4.1 *`), not drift to the padded column edge.
        let list = vec![
            row("12.10.0", false, false, None),
            row("12.4.1", true, true, Some("/home/u/.cuvm/versions/12.4.1")),
        ];
        let lines = format_text_rows(&list, false);
        assert_eq!(
            lines,
            [
                "12.10.0   <download available>",
                "12.4.1 *  /home/u/.cuvm/versions/12.4.1",
            ]
        );
    }
}
