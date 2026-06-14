//! Pure construction of the child-process environment for `cuvm exec` / `shell`.
//!
//! The Activators (`cuvm-platform`) render an [`EnvPlan`] as a *shell script* the
//! user's shell `eval`s. `exec`/`shell` instead launch a child process directly,
//! so they need the same activation as a key→value map applied to the child's
//! environment. This module is that mapping — pure, OS-aware, zero I/O (the
//! cuvm-core dependency rule), so the strip-then-prepend semantics are unit-
//! tested without spawning a shell.
//!
//! It mirrors the Activator contract exactly (spec §2.5/§8): strip any segment
//! recorded in the *current* `CUVM_INJECTED` breadcrumb FIRST (so re-`exec`ing
//! inside an already-active shell never double-prepends), then prepend the
//! plan's `bin`/`lib64` segments. Segments not in the breadcrumb survive — so
//! WSL's `/usr/lib/wsl/lib` is preserved without special-casing.

use crate::{EnvPlan, Os};

/// One environment-variable assignment to apply to a child process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

impl EnvVar {
    fn new(key: &str, value: impl Into<String>) -> Self {
        EnvVar {
            key: key.to_string(),
            value: value.into(),
        }
    }
}

/// The PATH-list separator for `os` (`;` on Windows, `:` elsewhere).
fn separator(os: Os) -> char {
    match os {
        Os::Windows => ';',
        Os::Linux => ':',
    }
}

/// Strip every segment of `current` that appears in `injected`, dropping empty
/// segments (the empty-segment guard that prevents a CWD-injection hazard), then
/// prepend `prepend`. Empty `prepend` segments are dropped too, so the joined
/// result can never contain an empty segment regardless of input.
///
/// On Windows the breadcrumb match is case-insensitive (paths are), mirroring
/// the PowerShell Activator's `-notcontains` (also case-insensitive) so a prior
/// injection recorded in one casing is still stripped when PATH carries another.
fn strip_then_prepend(
    current: &str,
    injected: &[&str],
    prepend: &[String],
    sep: char,
    windows: bool,
) -> String {
    let in_breadcrumb = |seg: &str| -> bool {
        if windows {
            injected.iter().any(|i| i.eq_ignore_ascii_case(seg))
        } else {
            injected.contains(&seg)
        }
    };
    let kept = current
        .split(sep)
        .filter(|seg| !seg.is_empty() && !in_breadcrumb(seg));
    prepend
        .iter()
        .map(String::as_str)
        .filter(|seg| !seg.is_empty())
        .chain(kept)
        .collect::<Vec<_>>()
        .join(&sep.to_string())
}

/// Build the full set of env-var assignments that activate `plan` in a child
/// process on `os`, given a reader for the *current* process environment.
///
/// `get(name)` returns the current value of an env var (`None` if unset). The
/// returned list is deterministic and mirrors the Activator's field order:
/// `CUDA_HOME`, `CUDA_PATH`, `CUDAToolkit_ROOT`, `PATH`, (`LD_LIBRARY_PATH` on
/// non-Windows), `CUVM_CURRENT`, `CUVM_INJECTED`.
///
/// `LD_LIBRARY_PATH` is emitted on Linux/WSL only; on Windows the library
/// search path is the same `PATH`, so the breadcrumb is `bin` alone — matching
/// each platform Activator.
#[must_use]
pub fn process_env(plan: &EnvPlan, os: Os, get: impl Fn(&str) -> Option<String>) -> Vec<EnvVar> {
    let sep = separator(os);
    let windows = matches!(os, Os::Windows);

    // The `EnvPlan` is OS-neutral and always uses `/` separators (it is built for
    // the Unix path); the Windows Activator normalizes them to `\`. Mirror that
    // here so the breadcrumb we record matches the backslash PATH entries the
    // Windows Activator writes — otherwise a later `use` could not strip them and
    // PATH would grow on every switch. No-op on Unix.
    let norm = |s: &str| -> String {
        if windows {
            s.replace('/', "\\")
        } else {
            s.to_string()
        }
    };
    let prepend_path: Vec<String> = plan.prepend_path.iter().map(|s| norm(s)).collect();
    let prepend_lib: Vec<String> = plan.prepend_lib.iter().map(|s| norm(s)).collect();

    // The breadcrumb to record == exactly what we prepend: bin (+ lib64 off-Windows).
    let mut new_injected: Vec<String> = prepend_path.clone();
    if !windows {
        new_injected.extend(prepend_lib.clone());
    }

    // Strip using the CURRENT breadcrumb, not the new one.
    let cur_injected = get("CUVM_INJECTED").unwrap_or_default();
    let injected_segs: Vec<&str> = cur_injected.split(sep).filter(|s| !s.is_empty()).collect();

    let cur_path = get("PATH").unwrap_or_default();
    let new_path = strip_then_prepend(&cur_path, &injected_segs, &prepend_path, sep, windows);

    let mut out = vec![
        EnvVar::new("CUDA_HOME", norm(&plan.cuda_home)),
        EnvVar::new("CUDA_PATH", norm(&plan.cuda_path)),
        EnvVar::new("CUDAToolkit_ROOT", norm(&plan.toolkit_root)),
        EnvVar::new("PATH", new_path),
    ];
    if !windows {
        let cur_lib = get("LD_LIBRARY_PATH").unwrap_or_default();
        let new_lib = strip_then_prepend(&cur_lib, &injected_segs, &prepend_lib, sep, windows);
        out.push(EnvVar::new("LD_LIBRARY_PATH", new_lib));
    }
    out.push(EnvVar::new("CUVM_CURRENT", plan.current.clone()));
    out.push(EnvVar::new(
        "CUVM_INJECTED",
        new_injected.join(&sep.to_string()),
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `EnvPlan` for a toolkit rooted at `root` (Unix-style paths).
    fn plan(root: &str) -> EnvPlan {
        EnvPlan {
            cuda_home: root.to_string(),
            cuda_path: root.to_string(),
            toolkit_root: root.to_string(),
            prepend_path: vec![format!("{root}/bin")],
            prepend_lib: vec![format!("{root}/lib64")],
            current: "12.4.1".to_string(),
            injected: vec![format!("{root}/bin"), format!("{root}/lib64")],
        }
    }

    /// Look up a key in a fixed `(key, value)` slice — a tiny env stand-in.
    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |k: &str| {
            owned
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
        }
    }

    fn value<'a>(vars: &'a [EnvVar], key: &str) -> Option<&'a str> {
        vars.iter().find(|v| v.key == key).map(|v| v.value.as_str())
    }

    #[test]
    fn fresh_env_prepends_and_sets_all_roots() {
        let p = plan("/home/u/.cuvm/versions/12.4.1");
        let vars = process_env(&p, Os::Linux, env_of(&[("PATH", "/usr/bin:/bin")]));
        assert_eq!(
            value(&vars, "CUDA_HOME"),
            Some("/home/u/.cuvm/versions/12.4.1")
        );
        assert_eq!(
            value(&vars, "CUDA_PATH"),
            Some("/home/u/.cuvm/versions/12.4.1")
        );
        assert_eq!(
            value(&vars, "CUDAToolkit_ROOT"),
            Some("/home/u/.cuvm/versions/12.4.1")
        );
        assert_eq!(
            value(&vars, "PATH"),
            Some("/home/u/.cuvm/versions/12.4.1/bin:/usr/bin:/bin")
        );
        assert_eq!(
            value(&vars, "LD_LIBRARY_PATH"),
            Some("/home/u/.cuvm/versions/12.4.1/lib64")
        );
        assert_eq!(value(&vars, "CUVM_CURRENT"), Some("12.4.1"));
        assert_eq!(
            value(&vars, "CUVM_INJECTED"),
            Some("/home/u/.cuvm/versions/12.4.1/bin:/home/u/.cuvm/versions/12.4.1/lib64")
        );
    }

    #[test]
    fn reactivation_strips_prior_breadcrumb_no_double_prepend() {
        // Already active on /old; switch to /new must remove the old segments.
        let p = plan("/new");
        let vars = process_env(
            &p,
            Os::Linux,
            env_of(&[
                ("PATH", "/old/bin:/usr/bin"),
                ("LD_LIBRARY_PATH", "/old/lib64:/lib"),
                ("CUVM_INJECTED", "/old/bin:/old/lib64"),
            ]),
        );
        assert_eq!(value(&vars, "PATH"), Some("/new/bin:/usr/bin"));
        assert_eq!(value(&vars, "LD_LIBRARY_PATH"), Some("/new/lib64:/lib"));
        // Idempotent: re-running with the NEW breadcrumb must not grow PATH.
        let again = process_env(
            &p,
            Os::Linux,
            env_of(&[
                ("PATH", "/new/bin:/usr/bin"),
                ("LD_LIBRARY_PATH", "/new/lib64:/lib"),
                ("CUVM_INJECTED", "/new/bin:/new/lib64"),
            ]),
        );
        assert_eq!(value(&again, "PATH"), Some("/new/bin:/usr/bin"));
    }

    #[test]
    fn wsl_driver_libs_are_preserved() {
        // /usr/lib/wsl/lib is never a breadcrumb member, so it must survive a switch.
        let p = plan("/new");
        let vars = process_env(
            &p,
            Os::Linux,
            env_of(&[
                ("LD_LIBRARY_PATH", "/usr/lib/wsl/lib:/old/lib64"),
                ("CUVM_INJECTED", "/old/bin:/old/lib64"),
            ]),
        );
        assert_eq!(
            value(&vars, "LD_LIBRARY_PATH"),
            Some("/new/lib64:/usr/lib/wsl/lib")
        );
    }

    #[test]
    fn empty_path_segments_are_dropped() {
        // A trailing colon would mean an empty segment == CWD on the search path.
        let p = plan("/new");
        let vars = process_env(&p, Os::Linux, env_of(&[("PATH", "/usr/bin:")]));
        assert_eq!(value(&vars, "PATH"), Some("/new/bin:/usr/bin"));
    }

    #[test]
    fn windows_uses_semicolons_no_ld_path_and_bin_only_breadcrumb() {
        // Realistic input: a backslash root (the Windows PathBuf) with the
        // `/bin` suffix that the OS-neutral `plan_for` appends. On Windows the
        // separator must be normalized to `\` so the breadcrumb matches the
        // backslash PATH entries the Windows Activator writes.
        let p = plan("C:\\Users\\u\\.cuvm\\versions\\12.4.1");
        let vars = process_env(
            &p,
            Os::Windows,
            env_of(&[("PATH", "C:\\Windows;C:\\Windows\\System32")]),
        );
        assert_eq!(
            value(&vars, "CUDA_HOME"),
            Some("C:\\Users\\u\\.cuvm\\versions\\12.4.1")
        );
        // Separator is `;`, the prepended `bin` is backslash-normalized.
        assert_eq!(
            value(&vars, "PATH"),
            Some("C:\\Users\\u\\.cuvm\\versions\\12.4.1\\bin;C:\\Windows;C:\\Windows\\System32")
        );
        // No LD_LIBRARY_PATH on Windows.
        assert_eq!(value(&vars, "LD_LIBRARY_PATH"), None);
        // Breadcrumb is bin only (the lib64 segment is Unix-only), backslash form.
        assert_eq!(
            value(&vars, "CUVM_INJECTED"),
            Some("C:\\Users\\u\\.cuvm\\versions\\12.4.1\\bin")
        );
    }

    #[test]
    fn windows_breadcrumb_strips_a_prior_backslash_injection() {
        // The breadcrumb we write (backslash) must round-trip: re-activating
        // with that exact CUVM_INJECTED in scope must not duplicate the entry.
        let p = plan("C:\\new");
        let vars = process_env(
            &p,
            Os::Windows,
            env_of(&[
                ("PATH", "C:\\new\\bin;C:\\Windows"),
                ("CUVM_INJECTED", "C:\\new\\bin"),
            ]),
        );
        assert_eq!(value(&vars, "PATH"), Some("C:\\new\\bin;C:\\Windows"));
    }

    #[test]
    fn windows_breadcrumb_strip_is_case_insensitive() {
        // Windows paths are case-insensitive (so is the PowerShell Activator's
        // -notcontains): a prior injection recorded in one casing must still be
        // stripped when PATH carries a differently-cased copy — else PATH grows.
        let p = plan("C:\\new");
        let vars = process_env(
            &p,
            Os::Windows,
            env_of(&[
                ("PATH", "c:\\OLD\\BIN;C:\\Windows"),
                ("CUVM_INJECTED", "C:\\old\\bin"),
            ]),
        );
        assert_eq!(value(&vars, "PATH"), Some("C:\\new\\bin;C:\\Windows"));
    }

    #[test]
    fn unix_breadcrumb_strip_stays_case_sensitive() {
        // On Unix, casing is significant: a differently-cased path is a DIFFERENT
        // directory and must be preserved, not mistaken for the breadcrumb.
        let p = plan("/new");
        let vars = process_env(
            &p,
            Os::Linux,
            env_of(&[("PATH", "/OLD/BIN:/usr/bin"), ("CUVM_INJECTED", "/old/bin")]),
        );
        assert_eq!(value(&vars, "PATH"), Some("/new/bin:/OLD/BIN:/usr/bin"));
    }

    #[test]
    fn missing_path_var_yields_just_the_prepend() {
        let p = plan("/new");
        let vars = process_env(&p, Os::Linux, |_| None);
        assert_eq!(value(&vars, "PATH"), Some("/new/bin"));
        assert_eq!(value(&vars, "LD_LIBRARY_PATH"), Some("/new/lib64"));
    }
}
