# cuvm Foundations + M1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build cuvm's foundational Cargo workspace and Milestone 1 — *adopt, switch, pin, and doctor* across Linux/WSL **and** Windows with **no downloading** — the first shippable version of the tool.

**Architecture:** Rust Cargo workspace, clean architecture: `cuvm-core` (pure domain, zero I/O) ← `cuvm-app` (use-cases + trait ports) ← leaf adapters (`cuvm-platform`, `cuvm-store`, `cuvm-registry`, `cuvm-download`, `cuvm-nvidia`); `cuvm-cli` is the composition root. Per-OS `Activator`/`Installer` live behind traits, dispatched at runtime; the binary *prints* an env script the shell `eval`s, with a `CUVM_INJECTED` breadcrumb for strip-before-prepend cleanup. Everything is TDD: failing test → run-fail → minimal impl → run-pass → commit.

**Tech Stack:** Rust 1.92 · clap (derive) · serde/serde_json · thiserror (core) + anyhow (app/cli) · ureq+rustls · sha2 · tar / lzma-rs / zip · `windows` crate · tests: insta, assert_cmd, predicates, mockall, tempfile, assert_fs, httpmock · CI cross-compile via cargo-zigbuild (linux/amd64, linux/arm64, windows/amd64).

**Source of truth:** the approved spec at `docs/superpowers/specs/2026-06-08-cuvm-implementation-design.md`. Treat its §2 (Verified Foundation) and §12 (compat tables) as facts — do not re-derive. Naming: command `cuvm`, home `~/.cuvm`, breadcrumb `CUVM_*`, crates `cuvm-*`; the repo dir/remote stays `cvm`.

## Work units in this plan

| WU | Title | Gates |
|---|---|---|
| WU-0 | Workspace scaffold + naming sweep + CI cross-compile | — |
| WU-1 | Core domain types + trait ports + runtime backend factory + stubs | — |
| WU-2 | Resolver + version-spec grammar | — |
| WU-3 | Manifest + Inventory state I/O | — |
| WU-4 | Linux adopt (scan + adopt-in-place) | relocatability ✓ |
| WU-5 | Linux Activator: env-script emission + `CUVM_INJECTED` cleanup | shim ✓ |
| WU-6 | Unix shims + hook + env/hook plumbing commands | shim ✓ |
| WU-7 | Compat engine + embedded data tables + nvidia-smi probe | compat ✓ (corrected) |
| WU-8 | M1 command wiring + doctor v1 | WU-2/3/5/7 |
| WU-9 | Windows backend (Activator + adopt + persistence) | shim ✓, compat ✓ |

**Build order:** WU-0 → WU-1 (serial foundations), then a Linux track (WU-2,3,5,6,7,8) and a Windows track (WU-9) proceed in parallel once WU-1's trait seam is on the branch. M1 ship-candidate checkpoint = after WU-8 (Linux) and WU-9 (Windows) are green: `adopt → default → use → pin → cd-hook switch → doctor` works on both OSes with no network.

**Out of scope (later plans):** M2 install/download (WU-10–15), M3 cuDNN (WU-16–18), M4 companions/polish (WU-19–21).

---

### WU-0: Workspace scaffold + naming sweep + CI cross-compile

**Goal.** Stand up the empty-but-buildable 8-crate Cargo workspace, pin the toolchain + cross targets, give `cuvm-cli` a minimal clap skeleton with snapshot-tested `--help`/`--version`, wire a GitHub Actions lane (fmt + clippy `-D warnings` + a `cargo-zigbuild` compile-all matrix for linux/amd64, linux/arm64, windows/amd64), and sweep `cvm`->`cuvm` / `~/.cvm`->`~/.cuvm` / `CVM_*`->`CUVM_*` in the legacy `CVM-*.md` docs (leaving the repo dir/remote `cvm` untouched). Gates: none. Every other WU builds on this scaffold.

**Conventions for this WU.** Every git command runs from the repo root `/home/daniel/cvm`. We start on `main`; create a working branch first (Task 0). All crates use `edition = "2021"` on stable 1.92. The empty leaf crates carry one `pub fn placeholder()` plus a unit test so they compile and `cargo fmt`/`clippy`/`test` have a target; real types arrive in later WUs.

---

#### Task 0 — Branch + workspace root + toolchain pin

**Files:**
- Create `/home/daniel/cvm/Cargo.toml`
- Create `/home/daniel/cvm/rust-toolchain.toml`
- Create `/home/daniel/cvm/.gitignore`

- [ ] **Step (branch):** Create and switch to the WU-0 branch so we never commit straight to `main`.
  ```bash
  cd /home/daniel/cvm && git checkout -b wu-0-workspace-scaffold
  ```
  Expected: `Switched to a new branch 'wu-0-workspace-scaffold'`.

- [ ] **Step (write the workspace root):** Create `/home/daniel/cvm/Cargo.toml` — a virtual manifest declaring all 8 members under `crates/` plus a `[workspace.dependencies]` table fixing shared versions (crate manifests opt in with `{ workspace = true }`).
  ```toml
  [workspace]
  resolver = "2"
  members = [
      "crates/cuvm-core",
      "crates/cuvm-app",
      "crates/cuvm-platform",
      "crates/cuvm-store",
      "crates/cuvm-registry",
      "crates/cuvm-download",
      "crates/cuvm-nvidia",
      "crates/cuvm-cli",
  ]

  [workspace.package]
  version = "0.0.0"
  edition = "2021"
  rust-version = "1.92"
  license = "MIT OR Apache-2.0"
  repository = "https://github.com/danghoangnhan/cvm"

  # Shared dependency versions (CHOSEN deps, spec §3.3). Later WUs add the rest.
  [workspace.dependencies]
  clap = { version = "4.5", features = ["derive"] }
  thiserror = "2.0"
  anyhow = "1.0"
  serde = { version = "1.0", features = ["derive"] }
  serde_json = "1.0"
  ureq = { version = "2.10", features = ["tls"] }
  sha2 = "0.10"
  tar = "0.4"
  lzma-rs = "0.3"
  zip = "2.2"
  time = { version = "0.3", features = ["serde", "formatting", "parsing"] }
  insta = "1.40"
  assert_cmd = "2.0"
  predicates = "3.1"
  mockall = "0.13"
  tempfile = "3.13"
  assert_fs = "1.1"
  httpmock = "0.7"

  [profile.release]
  lto = "thin"
  codegen-units = 1
  strip = true
  ```

- [ ] **Step (pin the toolchain + cross targets):** Create `/home/daniel/cvm/rust-toolchain.toml`. Pin stable plus the three cross targets the CI matrix builds (§3.3: static musl x86_64, linux arm64, windows-gnu via cargo-zigbuild).
  ```toml
  [toolchain]
  channel = "stable"
  components = ["rustfmt", "clippy"]
  targets = [
      "x86_64-unknown-linux-musl",
      "aarch64-unknown-linux-gnu",
      "x86_64-pc-windows-gnu",
  ]
  profile = "minimal"
  ```

- [ ] **Step (gitignore):** Create `/home/daniel/cvm/.gitignore`. Ignore build output + insta pending snapshots + editor cruft. Do NOT ignore `Cargo.lock` — this workspace ships a binary, so the lockfile is tracked.
  ```gitignore
  # Rust build artifacts
  /target/
  **/*.rs.bk

  # insta review queue (committed snapshots have no .new suffix)
  **/*.snap.new
  **/*.pending-snap

  # Editor / OS
  .DS_Store
  *.swp
  /.idea/
  /.vscode/
  ```

- [ ] **Step (verify the root parses):** With no member dirs yet, `cargo metadata` errors on the missing member manifests — expected, and it proves the root TOML itself parsed.
  ```bash
  cd /home/daniel/cvm && cargo metadata --no-deps --format-version 1 2>&1 | head -5
  ```
  Expected: an error mentioning failure to load `crates/cuvm-core/Cargo.toml` (member path resolved, file absent) — NOT a TOML parse error on the root.

- [ ] **Step (commit):**
  ```bash
  cd /home/daniel/cvm && git add Cargo.toml rust-toolchain.toml .gitignore && git commit -m "chore(workspace): scaffold virtual Cargo workspace root + toolchain pin

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 1 — The seven library crates (compile to empty)

**Files:**
- Create `/home/daniel/cvm/crates/cuvm-core/Cargo.toml`, `/home/daniel/cvm/crates/cuvm-core/src/lib.rs`
- Create `/home/daniel/cvm/crates/cuvm-app/Cargo.toml`, `/home/daniel/cvm/crates/cuvm-app/src/lib.rs`
- Create `/home/daniel/cvm/crates/cuvm-platform/Cargo.toml`, `/home/daniel/cvm/crates/cuvm-platform/src/lib.rs`
- Create `/home/daniel/cvm/crates/cuvm-store/Cargo.toml`, `/home/daniel/cvm/crates/cuvm-store/src/lib.rs`
- Create `/home/daniel/cvm/crates/cuvm-registry/Cargo.toml`, `/home/daniel/cvm/crates/cuvm-registry/src/lib.rs`
- Create `/home/daniel/cvm/crates/cuvm-download/Cargo.toml`, `/home/daniel/cvm/crates/cuvm-download/src/lib.rs`
- Create `/home/daniel/cvm/crates/cuvm-nvidia/Cargo.toml`, `/home/daniel/cvm/crates/cuvm-nvidia/src/lib.rs`

The dependency rule (§3.2) is encoded in these manifests: `cuvm-core` -> nothing internal; `cuvm-app` -> `cuvm-core` only; leaf adapters -> `cuvm-core`. No heavy deps are pulled before their WU. Each crate carries a `placeholder()` fn + unit test.

- [ ] **Step (cuvm-core manifest + lib):** Pure domain crate, zero I/O deps (§3.1); `thiserror` is allowed (its error enum lands in WU-2).

  `/home/daniel/cvm/crates/cuvm-core/Cargo.toml`:
  ```toml
  [package]
  name = "cuvm-core"
  version.workspace = true
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  repository.workspace = true

  [dependencies]
  thiserror = { workspace = true }
  ```

  `/home/daniel/cvm/crates/cuvm-core/src/lib.rs`:
  ```rust
  //! cuvm-core — pure domain types and logic. Zero I/O dependencies.
  //!
  //! Real types (`Version`, `Bundle`, `EnvPlan`, compat tables, ...) land in
  //! later work units. This placeholder keeps the crate building under WU-0.

  /// Scaffold marker. Replaced by real domain types in WU-2+.
  pub fn placeholder() -> &'static str {
      "cuvm-core"
  }

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn placeholder_names_the_crate() {
          assert_eq!(placeholder(), "cuvm-core");
      }
  }
  ```

- [ ] **Step (cuvm-app manifest + lib):** Use-cases + trait ports; depends only on `cuvm-core` (§3.2); `anyhow` at the app edge (§3.3).

  `/home/daniel/cvm/crates/cuvm-app/Cargo.toml`:
  ```toml
  [package]
  name = "cuvm-app"
  version.workspace = true
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  repository.workspace = true

  [dependencies]
  cuvm-core = { path = "../cuvm-core" }
  anyhow = { workspace = true }
  ```

  `/home/daniel/cvm/crates/cuvm-app/src/lib.rs`:
  ```rust
  //! cuvm-app — use-cases and trait ports (Resolver, Activator, Installer, ...).
  //!
  //! Trait ports land in WU-1. This placeholder keeps the crate building and
  //! asserts the core dependency edge is wired.

  /// Scaffold marker. Replaced by trait ports in WU-1.
  pub fn placeholder() -> String {
      format!("cuvm-app over {}", cuvm_core::placeholder())
  }

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn placeholder_wraps_core() {
          assert_eq!(placeholder(), "cuvm-app over cuvm-core");
      }
  }
  ```

- [ ] **Step (the five leaf adapter crates):** Each depends on `cuvm-core` only (cross-edges like `registry -> download` arrive in their own WUs). Write all five; they are identical in shape.

  `/home/daniel/cvm/crates/cuvm-platform/Cargo.toml`:
  ```toml
  [package]
  name = "cuvm-platform"
  version.workspace = true
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  repository.workspace = true

  [dependencies]
  cuvm-core = { path = "../cuvm-core" }
  ```
  `/home/daniel/cvm/crates/cuvm-platform/src/lib.rs`:
  ```rust
  //! cuvm-platform — per-OS Activator + Installer backends.
  //!
  //! `#[cfg(unix)]` / `#[cfg(windows)]` syscall floors and the
  //! `new_activator` / `new_installer` runtime factories land in WU-1+.

  /// Scaffold marker. Replaced by per-OS backends in WU-1+.
  pub fn placeholder() -> &'static str {
      "cuvm-platform"
  }

  #[cfg(test)]
  mod tests {
      #[test]
      fn placeholder_names_the_crate() {
          assert_eq!(super::placeholder(), "cuvm-platform");
      }
  }
  ```

  `/home/daniel/cvm/crates/cuvm-store/Cargo.toml`:
  ```toml
  [package]
  name = "cuvm-store"
  version.workspace = true
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  repository.workspace = true

  [dependencies]
  cuvm-core = { path = "../cuvm-core" }
  ```
  `/home/daniel/cvm/crates/cuvm-store/src/lib.rs`:
  ```rust
  //! cuvm-store — atomic manifest/.cuvm-meta I/O + content-addressed cudnn store.
  //!
  //! Real I/O lands in WU-3. WU-0 placeholder only.

  /// Scaffold marker. Replaced by atomic store I/O in WU-3.
  pub fn placeholder() -> &'static str {
      "cuvm-store"
  }

  #[cfg(test)]
  mod tests {
      #[test]
      fn placeholder_names_the_crate() {
          assert_eq!(super::placeholder(), "cuvm-store");
      }
  }
  ```

  `/home/daniel/cvm/crates/cuvm-registry/Cargo.toml`:
  ```toml
  [package]
  name = "cuvm-registry"
  version.workspace = true
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  repository.workspace = true

  [dependencies]
  cuvm-core = { path = "../cuvm-core" }
  ```
  `/home/daniel/cvm/crates/cuvm-registry/src/lib.rs`:
  ```rust
  //! cuvm-registry — parse redistrib_<ver>.json (serde flatten, dynamic keys).
  //!
  //! Real parser lands in WU-10. WU-0 placeholder only.

  /// Scaffold marker. Replaced by the redist parser in WU-10.
  pub fn placeholder() -> &'static str {
      "cuvm-registry"
  }

  #[cfg(test)]
  mod tests {
      #[test]
      fn placeholder_names_the_crate() {
          assert_eq!(super::placeholder(), "cuvm-registry");
      }
  }
  ```

  `/home/daniel/cvm/crates/cuvm-download/Cargo.toml`:
  ```toml
  [package]
  name = "cuvm-download"
  version.workspace = true
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  repository.workspace = true

  [dependencies]
  cuvm-core = { path = "../cuvm-core" }
  ```
  `/home/daniel/cvm/crates/cuvm-download/src/lib.rs`:
  ```rust
  //! cuvm-download — ureq+rustls fetch, sha256, tar.xz / zip extract (zip-slip guard).
  //!
  //! Real downloader/extractor lands in WU-11/WU-12. WU-0 placeholder only.

  /// Scaffold marker. Replaced by the downloader in WU-11.
  pub fn placeholder() -> &'static str {
      "cuvm-download"
  }

  #[cfg(test)]
  mod tests {
      #[test]
      fn placeholder_names_the_crate() {
          assert_eq!(super::placeholder(), "cuvm-download");
      }
  }
  ```

  `/home/daniel/cvm/crates/cuvm-nvidia/Cargo.toml`:
  ```toml
  [package]
  name = "cuvm-nvidia"
  version.workspace = true
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  repository.workspace = true

  [dependencies]
  cuvm-core = { path = "../cuvm-core" }
  ```
  `/home/daniel/cvm/crates/cuvm-nvidia/src/lib.rs`:
  ```rust
  //! cuvm-nvidia — nvidia-smi driver probe (graceful-absent).
  //!
  //! Real DriverProbe impl lands in WU-7-adjacent work. WU-0 placeholder only.

  /// Scaffold marker. Replaced by the nvidia-smi probe later.
  pub fn placeholder() -> &'static str {
      "cuvm-nvidia"
  }

  #[cfg(test)]
  mod tests {
      #[test]
      fn placeholder_names_the_crate() {
          assert_eq!(super::placeholder(), "cuvm-nvidia");
      }
  }
  ```

- [ ] **Step (run the per-crate tests, see them pass):** `cuvm-cli` does not exist yet, so build/test only the seven libraries.
  ```bash
  cd /home/daniel/cvm && cargo test -p cuvm-core -p cuvm-app -p cuvm-platform -p cuvm-store -p cuvm-registry -p cuvm-download -p cuvm-nvidia 2>&1 | tail -20
  ```
  Expected: each crate compiles; output shows seven `test result: ok. 1 passed; 0 failed` lines.

- [ ] **Step (prove core has no internal deps — §3.1 invariant):**
  ```bash
  cd /home/daniel/cvm && cargo tree -p cuvm-core -e normal --depth 1
  ```
  Expected: `cuvm-core` with `thiserror` as its only child; NO `cuvm-*` entry beneath it.

- [ ] **Step (commit):**
  ```bash
  cd /home/daniel/cvm && git add crates/cuvm-core crates/cuvm-app crates/cuvm-platform crates/cuvm-store crates/cuvm-registry crates/cuvm-download crates/cuvm-nvidia && git commit -m "chore(crates): scaffold seven library crates with dependency-rule edges

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 2 — `cuvm-cli` clap skeleton: `--version` (TDD)

**Files:**
- Create `/home/daniel/cvm/crates/cuvm-cli/Cargo.toml`
- Create `/home/daniel/cvm/crates/cuvm-cli/src/cli.rs`
- Create `/home/daniel/cvm/crates/cuvm-cli/src/main.rs`
- Create `/home/daniel/cvm/crates/cuvm-cli/tests/cli_help.rs`

The binary is the composition root (§3.2). WU-0 only needs `--version`/`--help`; the full command tree (§7) lands in WU-1/WU-8. Drive it test-first with `assert_cmd`.

- [ ] **Step (cli-binary manifest):** `cuvm-cli` may depend on everything (§3.2); for WU-0 it depends on `cuvm-app` (+transitive core) and `clap`/`anyhow`, with dev-deps for e2e + snapshot tests. The binary is named `cuvm`.

  `/home/daniel/cvm/crates/cuvm-cli/Cargo.toml`:
  ```toml
  [package]
  name = "cuvm-cli"
  version.workspace = true
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  repository.workspace = true

  [[bin]]
  name = "cuvm"
  path = "src/main.rs"

  [dependencies]
  cuvm-app = { path = "../cuvm-app" }
  clap = { workspace = true }
  anyhow = { workspace = true }

  [dev-dependencies]
  assert_cmd = { workspace = true }
  predicates = { workspace = true }
  insta = { workspace = true }
  ```

- [ ] **Step (write the failing test for `--version`):** Create the e2e test first; it asserts `cuvm --version` succeeds and prints a line starting with `cuvm `. Fails to compile (no binary yet) = the failing state.

  `/home/daniel/cvm/crates/cuvm-cli/tests/cli_help.rs`:
  ```rust
  use assert_cmd::Command;
  use predicates::prelude::*;

  /// `cuvm --version` prints `cuvm <semver>` and exits 0.
  #[test]
  fn version_flag_prints_name_and_version() {
      Command::cargo_bin("cuvm")
          .expect("binary `cuvm` is built")
          .arg("--version")
          .assert()
          .success()
          .stdout(predicate::str::starts_with("cuvm "));
  }
  ```

- [ ] **Step (run it, see it fail):**
  ```bash
  cd /home/daniel/cvm && cargo test -p cuvm-cli --test cli_help version_flag 2>&1 | tail -15
  ```
  Expected: failure — a compile error (`couldn't read .../src/main.rs`) or, once partially present, `assert_cmd` panics with `Unable to find ... cargo_bin("cuvm")`. Not `ok`.

- [ ] **Step (minimal implementation — clap derive + main):** `#[command(version, ...)]` auto-derives `--version`/`--help` from the crate version.

  `/home/daniel/cvm/crates/cuvm-cli/src/cli.rs`:
  ```rust
  //! cuvm command-line surface (clap derive). WU-0: root parser with
  //! `--version` / `--help` only; subcommands (§7) land in WU-1/WU-8.

  use clap::Parser;

  /// cuvm — a CUDA toolkit version manager (nvm for CUDA).
  #[derive(Debug, Parser)]
  #[command(
      name = "cuvm",
      version,
      about = "cuvm — a CUDA toolkit version manager (nvm for CUDA).",
      long_about = None
  )]
  pub struct Cli {}

  impl Cli {
      /// Parse process args into the root CLI. Exits the process on
      /// `--help` / `--version` / parse error (clap's standard behavior).
      pub fn parse_args() -> Self {
          Cli::parse()
      }
  }
  ```

  `/home/daniel/cvm/crates/cuvm-cli/src/main.rs`:
  ```rust
  //! cuvm binary — composition root. WU-0: parse the root CLI; with no
  //! subcommands yet there is nothing to dispatch, so success is a no-op.

  mod cli;

  use anyhow::Result;
  use cli::Cli;

  fn main() -> Result<()> {
      let _args = Cli::parse_args();
      // Subcommand dispatch lands in WU-1/WU-8. --version/--help are handled
      // by clap before this point, so an argless invocation is a no-op.
      Ok(())
  }
  ```

- [ ] **Step (run the test, see it pass):**
  ```bash
  cd /home/daniel/cvm && cargo test -p cuvm-cli --test cli_help version_flag 2>&1 | tail -10
  ```
  Expected: `test version_flag_prints_name_and_version ... ok` and `test result: ok. 1 passed; 0 failed`.

- [ ] **Step (eyeball the version string):**
  ```bash
  cd /home/daniel/cvm && cargo run -q -p cuvm-cli -- --version
  ```
  Expected: `cuvm 0.0.0`.

- [ ] **Step (commit):**
  ```bash
  cd /home/daniel/cvm && git add crates/cuvm-cli && git commit -m "feat(cli): clap skeleton with --version

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 3 — `cuvm --help` golden snapshot (insta, TDD)

**Files:**
- Modify `/home/daniel/cvm/crates/cuvm-cli/tests/cli_help.rs`
- Create `/home/daniel/cvm/crates/cuvm-cli/tests/snapshots/cli_help__help.snap`

Pinning help text now means any later command-surface change (WU-1/WU-8) appears as a reviewable diff.

- [ ] **Step (add the failing snapshot test):** Append to `/home/daniel/cvm/crates/cuvm-cli/tests/cli_help.rs`. With no committed `.snap`, insta fails (snapshot missing).
  ```rust
  /// `cuvm --help` output is pinned by a golden snapshot so any change to the
  /// command surface (subcommands added in later WUs) is a reviewable diff.
  #[test]
  fn help_output_matches_snapshot() {
      let output = Command::cargo_bin("cuvm")
          .expect("binary `cuvm` is built")
          .arg("--help")
          .output()
          .expect("run cuvm --help");
      assert!(output.status.success(), "cuvm --help should exit 0");
      let stdout = String::from_utf8(output.stdout).expect("help text is utf-8");
      insta::assert_snapshot!("help", stdout);
  }
  ```

- [ ] **Step (run it, see it fail):**
  ```bash
  cd /home/daniel/cvm && cargo test -p cuvm-cli --test cli_help help_output 2>&1 | tail -15
  ```
  Expected: failure — insta reports an undefined snapshot (`help_output_matches_snapshot ... FAILED`; message about snapshot `help` NOT FOUND; a `.snap.new` is written).

- [ ] **Step (accept the snapshot):** Render the canonical help text into the committed snapshot. Use the insta CLI if present, else rename the `.snap.new`.
  ```bash
  cd /home/daniel/cvm && cargo insta accept 2>/dev/null || find crates/cuvm-cli/tests/snapshots -name '*.snap.new' -exec sh -c 'mv "$1" "${1%.new}"' _ {} \;
  ```
  Expected: `crates/cuvm-cli/tests/snapshots/cli_help__help.snap` now exists; its body (after the insta header) reads approximately:
  ```text
  cuvm — a CUDA toolkit version manager (nvm for CUDA).

  Usage: cuvm

  Options:
    -h, --help     Print help
    -V, --version  Print version
  ```

- [ ] **Step (re-run, see it pass):**
  ```bash
  cd /home/daniel/cvm && cargo test -p cuvm-cli --test cli_help 2>&1 | tail -10
  ```
  Expected: `test result: ok. 2 passed; 0 failed`; no `.snap.new` remain (`find crates -name '*.snap.new'` prints nothing).

- [ ] **Step (commit):**
  ```bash
  cd /home/daniel/cvm && git add crates/cuvm-cli/tests && git commit -m "test(cli): golden snapshot of cuvm --help

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 4 — Workspace builds clean: fmt + clippy `-D warnings`

**Files:** none (verification + minimal fixups only).

The brief's quality gate: `cargo fmt --check` and `clippy -D warnings` clean across the whole workspace, so CI (Task 6) is green on first push.

- [ ] **Step (format the workspace):**
  ```bash
  cd /home/daniel/cvm && cargo fmt --all
  ```
  Expected: no output (files conform / are now normalized).

- [ ] **Step (verify fmt is a no-op — the CI check):**
  ```bash
  cd /home/daniel/cvm && cargo fmt --all -- --check
  ```
  Expected: exit 0, no diff. If a diff appears, re-run `cargo fmt --all` and re-check.

- [ ] **Step (run clippy as CI will, deny warnings):**
  ```bash
  cd /home/daniel/cvm && cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -20
  ```
  Expected: `Finished` with no warnings/errors. If clippy flags anything (e.g. a needless `format!`), fix it minimally and re-run until clean.

- [ ] **Step (full workspace test sweep):**
  ```bash
  cd /home/daniel/cvm && cargo test --workspace 2>&1 | tail -20
  ```
  Expected: every crate `test result: ok`; zero failures across the workspace.

- [ ] **Step (commit lockfile + any fixups):** First commit of the generated `Cargo.lock` (binary workspace -> lockfile tracked).
  ```bash
  cd /home/daniel/cvm && git add -A && git commit -m "chore(workspace): fmt + clippy clean; commit Cargo.lock

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 5 — `justfile` task runner

**Files:**
- Create `/home/daniel/cvm/justfile`

One canonical name per task, mirroring exactly what CI runs.

- [ ] **Step (write the justfile):**
  ```make
  # cuvm task runner. Run `just` to list recipes.
  # Cross-compile recipes require `cargo-zigbuild` + `ziglang` (see CI).

  default:
      @just --list

  # Format check (CI gate)
  fmt:
      cargo fmt --all -- --check

  # Auto-format in place
  fmt-fix:
      cargo fmt --all

  # Lint, deny warnings (CI gate)
  clippy:
      cargo clippy --workspace --all-targets -- -D warnings

  # Run the whole test suite
  test:
      cargo test --workspace

  # Review/accept pending insta snapshots
  snapshots:
      cargo insta review

  # Native debug build
  build:
      cargo build --workspace

  # Native release build of the cuvm binary
  release:
      cargo build -p cuvm-cli --release

  # --- cross compile (cargo-zigbuild), mirrors the CI compile-all matrix ---
  build-linux-amd64:
      cargo zigbuild -p cuvm-cli --release --target x86_64-unknown-linux-musl

  build-linux-arm64:
      cargo zigbuild -p cuvm-cli --release --target aarch64-unknown-linux-gnu

  build-windows-amd64:
      cargo zigbuild -p cuvm-cli --release --target x86_64-pc-windows-gnu

  # Build all three release targets
  build-all: build-linux-amd64 build-linux-arm64 build-windows-amd64

  # Full local gate before pushing
  ci: fmt clippy test
  ```

- [ ] **Step (verify recipes parse / file present):** Do not block WU-0 on a `just` install; CI uses the raw commands too.
  ```bash
  cd /home/daniel/cvm && (just --list 2>/dev/null || (test -f justfile && echo "justfile present (install 'just' to run recipes)"))
  ```
  Expected: the recipe list, or `justfile present (...)`.

- [ ] **Step (run the local gate if just is available):**
  ```bash
  cd /home/daniel/cvm && just ci 2>/dev/null || echo "skip: install 'just' (CI runs fmt+clippy+test directly)"
  ```
  Expected: clean fmt+clippy+test, or the skip line.

- [ ] **Step (commit):**
  ```bash
  cd /home/daniel/cvm && git add justfile && git commit -m "chore(dev): add justfile task runner

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 6 — GitHub Actions: fmt + clippy + compile-all matrix

**Files:**
- Create `/home/daniel/cvm/.github/workflows/ci.yml`

A `lint` job (fmt-check + clippy `-D warnings` + full test on linux/amd64) plus a `compile-all` matrix cross-building the `cuvm` binary for linux/amd64 (musl), linux/arm64, and windows/amd64 (gnu) via `cargo-zigbuild` from a single Ubuntu runner (§3.3).

- [ ] **Step (write the workflow):**
  ```yaml
  name: ci

  on:
    push:
      branches: [main]
    pull_request:

  env:
    CARGO_TERM_COLOR: always
    RUSTFLAGS: "-D warnings"

  jobs:
    lint:
      name: fmt + clippy + test (linux/amd64)
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v4
        - name: Install Rust (stable + components)
          uses: dtolnay/rust-toolchain@stable
          with:
            components: rustfmt, clippy
        - uses: Swatinem/rust-cache@v2
        - name: fmt --check
          run: cargo fmt --all -- --check
        - name: clippy -D warnings
          run: cargo clippy --workspace --all-targets -- -D warnings
        - name: test
          run: cargo test --workspace

    compile-all:
      name: compile ${{ matrix.name }}
      runs-on: ubuntu-latest
      strategy:
        fail-fast: false
        matrix:
          include:
            - name: linux/amd64
              target: x86_64-unknown-linux-musl
            - name: linux/arm64
              target: aarch64-unknown-linux-gnu
            - name: windows/amd64
              target: x86_64-pc-windows-gnu
      steps:
        - uses: actions/checkout@v4
        - name: Install Rust (stable + target)
          uses: dtolnay/rust-toolchain@stable
          with:
            targets: ${{ matrix.target }}
        - uses: Swatinem/rust-cache@v2
          with:
            key: ${{ matrix.target }}
        - name: Install Zig
          uses: mlugg/setup-zig@v1
        - name: Install cargo-zigbuild
          run: cargo install cargo-zigbuild --locked
        - name: cross compile cuvm binary
          run: cargo zigbuild -p cuvm-cli --release --target ${{ matrix.target }}
        - name: Upload binary
          uses: actions/upload-artifact@v4
          with:
            name: cuvm-${{ matrix.target }}
            path: |
              target/${{ matrix.target }}/release/cuvm
              target/${{ matrix.target }}/release/cuvm.exe
            if-no-files-found: warn
  ```

- [ ] **Step (lint the workflow YAML locally):** Use Python's always-present YAML parser rather than installing `actionlint`.
  ```bash
  cd /home/daniel/cvm && python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); print('ci.yml: valid YAML')"
  ```
  Expected: `ci.yml: valid YAML`.

- [ ] **Step (optional local cross-build sanity):** Confirm the windows-gnu cross links the skeleton if tooling is present; skips cleanly otherwise (CI installs it).
  ```bash
  cd /home/daniel/cvm && (command -v cargo-zigbuild >/dev/null && cargo zigbuild -p cuvm-cli --release --target x86_64-pc-windows-gnu 2>&1 | tail -5) || echo "skip: cargo-zigbuild not installed locally (CI installs it)"
  ```
  Expected: `Finished release [...] target(s)` producing `target/x86_64-pc-windows-gnu/release/cuvm.exe`, or the skip line.

- [ ] **Step (commit):**
  ```bash
  cd /home/daniel/cvm && git add .github/workflows/ci.yml && git commit -m "ci: fmt + clippy + cargo-zigbuild compile-all matrix (linux amd64/arm64, windows amd64)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 7 — Naming sweep: `cvm`->`cuvm` in the legacy docs

**Files:**
- Modify `/home/daniel/cvm/CVM-Design-Document.md`
- Modify `/home/daniel/cvm/CVM-ADRs.md`
- Modify `/home/daniel/cvm/CUDA-Version-Manager-Research.md`

Per §1: code/command/home/breadcrumbs use `cuvm`/`CUVM_*`; sweep the legacy doc set `cvm`->`cuvm`, `~/.cvm`->`~/.cuvm`, `CVM_*`->`CUVM_*`, `.cvm-meta`->`.cuvm-meta`. **The repo dir and remote URL stay `cvm`** — so any literal `github.com/danghoangnhan/cvm` must NOT be rewritten. Only these three legacy `CVM-*.md` files are swept; the approved spec already uses `cuvm` and is left alone.

- [ ] **Step (snapshot the pre-sweep counts):**
  ```bash
  cd /home/daniel/cvm && grep -roiE 'cvm|CVM_|\.cvm/|\.cvm-meta|CVM_HOME' CVM-Design-Document.md CVM-ADRs.md CUDA-Version-Manager-Research.md | wc -l
  ```
  Expected: a positive count (~113 across the three files per the initial scan). Note it for comparison.

- [ ] **Step (run the ordered substitution):** Most-specific patterns first so nothing is double-rewritten. The remote URL is protected by a placeholder restored at the end. Word boundaries (`\bcvm\b`, `\bCVM\b`) avoid touching substrings; case-specific rules handle `CVM` vs `cvm`.
  ```bash
  cd /home/daniel/cvm && for f in CVM-Design-Document.md CVM-ADRs.md CUDA-Version-Manager-Research.md; do
    sed -i -E \
      -e 's#github\.com/danghoangnhan/cvm#__KEEP_REMOTE__#g' \
      -e 's/\.cvm-meta/.cuvm-meta/g' \
      -e 's#~/\.cvm#~/.cuvm#g' \
      -e 's/\bCVM_([A-Z]+)\b/CUVM_\1/g' \
      -e 's/\bCVM\b/CUVM/g' \
      -e 's/\bcvm\b/cuvm/g' \
      -e 's#__KEEP_REMOTE__#github.com/danghoangnhan/cvm#g' \
      "$f"
  done
  echo "sweep applied"
  ```
  Expected: `sweep applied`.

- [ ] **Step (verify: no stray standalone tokens; remote preserved):**
  ```bash
  cd /home/daniel/cvm && echo "== residual cvm/CVM tokens (expect only the remote URL) ==" && grep -nE '\bcvm\b|\bCVM\b|CVM_|\.cvm/' CVM-Design-Document.md CVM-ADRs.md CUDA-Version-Manager-Research.md ; echo "== remote URL intact? ==" && grep -n 'danghoangnhan/cvm' CVM-Design-Document.md CVM-ADRs.md CUDA-Version-Manager-Research.md
  ```
  Expected: the residual grep prints only lines where `cvm` is part of `github.com/danghoangnhan/cvm` (the intentionally-preserved remote); the remote grep confirms those URLs survived. If any other standalone `cvm`/`CVM_` survives, extend the sed rule (e.g. an unanticipated form like `CVM-`) and re-run.

- [ ] **Step (spot-check key swept lines):** Confirm command examples + home dir + breadcrumb names now read `cuvm`/`~/.cuvm`/`CUVM_*`.
  ```bash
  cd /home/daniel/cvm && grep -nE 'cuvm use|cuvm install|~/\.cuvm|CUVM_HOME|\.cuvm-meta' CVM-Design-Document.md | head -10
  ```
  Expected: lines like `cuvm use <spec>`, `cuvm install <ver> ...`, `~/.cuvm`, `CUVM_HOME`, `.cuvm-meta.json` — proving the sweep hit commands, the home dir, breadcrumbs, and the sidecar filename.

- [ ] **Step (commit):**
  ```bash
  cd /home/daniel/cvm && git add CVM-Design-Document.md CVM-ADRs.md CUDA-Version-Manager-Research.md && git commit -m "docs: sweep cvm->cuvm, ~/.cvm->~/.cuvm, CVM_*->CUVM_* (keep repo/remote name)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 8 — Final green gate + push

**Files:** none.

- [ ] **Step (full clean rebuild + test from scratch):** Prove the committed tree builds and tests green end to end.
  ```bash
  cd /home/daniel/cvm && cargo clean && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace 2>&1 | tail -25
  ```
  Expected: fmt no-op, clippy `Finished` no warnings, and all workspace tests `ok` (7 lib unit tests + 2 cli e2e/snapshot tests).

- [ ] **Step (confirm tree is clean, nothing uncommitted):**
  ```bash
  cd /home/daniel/cvm && git status --porcelain
  ```
  Expected: empty output (every artifact from Tasks 0-7 is committed).

- [ ] **Step (push the branch + open PR):**
  ```bash
  cd /home/daniel/cvm && git push -u origin wu-0-workspace-scaffold && gh pr create --fill --title "WU-0: workspace scaffold + naming sweep + CI cross-compile" --body "Scaffolds the 8-crate Cargo workspace (dependency rule encoded per spec §3.2), pins stable 1.92 + cross targets, adds a clap \`cuvm\` skeleton with snapshot-tested \`--help\`/\`--version\`, a justfile, the fmt+clippy+zigbuild compile-all CI matrix (linux amd64/arm64 + windows amd64), and sweeps cvm->cuvm in the legacy docs (repo/remote name preserved). Gates: none.

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
  ```
  Expected: branch pushed; PR URL printed. CI runs the `lint` job and the three-target `compile-all` matrix — all green is the WU-0 done condition.

---

### WU-1: Core domain types + trait ports + runtime backend factory + stubs

**Goal.** Land the *seam* that lets the Linux and Windows tracks proceed in parallel: every CONTRACT core type in `cuvm-core`, every trait port in `cuvm-app`, and a runtime factory in `cuvm-platform` (`new_activator`/`new_installer`) returning `Box<dyn ..>` backed by unix/windows stub structs whose methods all return a `NotImplemented` error. No behavior beyond construction, ordering, serde round-trip of the value types, and factory dispatch.

**Pre-conditions (from WU-0).** A `[workspace]` exists at `/home/daniel/cvm/Cargo.toml`, `rust-toolchain.toml` pins stable 1.92 with gnu target, and empty crate skeletons exist under `crates/`. This WU edits the three crate `Cargo.toml`s to add deps + wire workspace members, then fills the source. All commands run from `/home/daniel/cvm`. **Gates: none.**

> Backend dispatch is **runtime** (so both backends compile on every host and the Windows golden tests run on Linux CI). The only `#[cfg]` in this WU is at the module floor: `unix.rs`/`windows.rs` are *both* compiled everywhere (they are plain modules, not `#[cfg]`-gated), because they contain zero syscalls in WU-1 — they only return `NotImplemented`. The `#[cfg]` syscall floor arrives in WU-5/WU-9.

---

#### Task 1.1 — Wire the three crate manifests + workspace deps

**Files:**
- Modify: `/home/daniel/cvm/Cargo.toml` (workspace members + `[workspace.dependencies]`)
- Modify: `/home/daniel/cvm/crates/cuvm-core/Cargo.toml`
- Modify: `/home/daniel/cvm/crates/cuvm-app/Cargo.toml`
- Modify: `/home/daniel/cvm/crates/cuvm-platform/Cargo.toml`

- [ ] **Step:** Add the dependency-rule-respecting members and shared dep versions to the root `Cargo.toml`. Set the `[workspace.dependencies]` block (single source of version truth; leaf crates reference `.workspace = true`):

  ```toml
  [workspace]
  resolver = "2"
  members = ["crates/cuvm-core", "crates/cuvm-app", "crates/cuvm-platform"]

  [workspace.package]
  edition = "2021"
  rust-version = "1.92"
  license = "MIT"
  repository = "https://github.com/danghoangnhan/cvm"

  [workspace.dependencies]
  thiserror = "2"
  anyhow = "1"
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  time = { version = "0.3", features = ["serde", "formatting", "parsing", "macros"] }
  # internal crates (path deps; the compiler enforces the Dependency Rule)
  cuvm-core = { path = "crates/cuvm-core" }
  cuvm-app = { path = "crates/cuvm-app" }
  ```

  > Other workspace members (`cuvm-store`, `cuvm-registry`, `cuvm-download`, `cuvm-nvidia`, `cuvm-cli`) are added in their own WUs; this WU only needs these three.

- [ ] **Step:** Set `crates/cuvm-core/Cargo.toml` — pure domain, **zero I/O deps**. Only `thiserror` (errors), `serde`/`serde_json` (manifest value types), and `time` (`OffsetDateTime`). No http, no fs, no async.

  ```toml
  [package]
  name = "cuvm-core"
  version = "0.1.0"
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  repository.workspace = true

  [dependencies]
  thiserror.workspace = true
  serde.workspace = true
  serde_json.workspace = true
  time.workspace = true
  ```

- [ ] **Step:** Set `crates/cuvm-app/Cargo.toml` — use-cases + trait ports; **depends only on `cuvm-core`** (plus `anyhow` for the edge `Result`).

  ```toml
  [package]
  name = "cuvm-app"
  version = "0.1.0"
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  repository.workspace = true

  [dependencies]
  cuvm-core.workspace = true
  anyhow.workspace = true
  ```

- [ ] **Step:** Set `crates/cuvm-platform/Cargo.toml` — the factory + stubs; depends on `cuvm-core` (types) and `cuvm-app` (the trait ports it implements).

  ```toml
  [package]
  name = "cuvm-platform"
  version = "0.1.0"
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  repository.workspace = true

  [dependencies]
  cuvm-core.workspace = true
  cuvm-app.workspace = true
  anyhow.workspace = true
  ```

- [ ] **Step:** Confirm the workspace resolves and the empty crates still build (no source yet beyond WU-0 stubs). Run:

  ```bash
  cargo metadata --no-deps --format-version 1 >/dev/null && cargo build --workspace
  ```

  **Expected:** `cargo metadata` exits 0 (the three members + dep graph parse), and `cargo build --workspace` finishes with `Finished` and no dependency-rule violation. If `cuvm-app` accidentally pulled an I/O crate, the build still passes here — the rule is enforced structurally by which crates are *listed*, which we just constrained.

- [ ] **Step:** Commit.

  ```bash
  git add Cargo.toml crates/cuvm-core/Cargo.toml crates/cuvm-app/Cargo.toml crates/cuvm-platform/Cargo.toml
  git commit -m "build(workspace): wire cuvm-core/app/platform manifests and shared deps

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 1.2 — `Version` (numeric tuple ordering + parse)

**Files:**
- Create: `crates/cuvm-core/src/version.rs`
- Modify: `crates/cuvm-core/src/lib.rs` (declare module + re-export)
- Test: inline `#[cfg(test)]` in `crates/cuvm-core/src/version.rs`

- [ ] **Step:** Write the failing test. The CONTRACT requires field-by-field **numeric** compare with missing tail = 0 (drivers are 3-part, `cccl` is 4+-part — see spec §2.1/§4). This is the single highest-value invariant in core; lexical compare is explicitly forbidden by spec §2.4 ("never lexically"). Put this test block at the bottom of `version.rs`:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn parse_extracts_numeric_fields_and_keeps_raw() {
          let v = Version::parse("13.3.0").unwrap();
          assert_eq!(v.fields, vec![13, 3, 0]);
          assert_eq!(v.raw, "13.3.0");
          assert_eq!(v.major(), 13);
      }

      #[test]
      fn parse_supports_four_part_cccl_version() {
          let v = Version::parse("13.3.3.3.1").unwrap();
          assert_eq!(v.fields, vec![13, 3, 3, 3, 1]);
          assert_eq!(v.major(), 13);
      }

      #[test]
      fn parse_rejects_empty_and_non_numeric() {
          assert!(Version::parse("").is_err());
          assert!(Version::parse("12.x").is_err());
          assert!(Version::parse("v12.4").is_err());
      }

      #[test]
      fn ord_is_numeric_not_lexical() {
          // 570.26 < 570.124.06 numerically; lexical compare would get this WRONG.
          let a = Version::parse("570.26").unwrap();
          let b = Version::parse("570.124.06").unwrap();
          assert!(a < b, "expected 570.26 < 570.124.06 numerically");
      }

      #[test]
      fn ord_treats_missing_tail_as_zero() {
          // 12.4 == 12.4.0 ; 12.4 < 12.4.1
          assert_eq!(Version::parse("12.4").unwrap(), Version::parse("12.4.0").unwrap());
          assert!(Version::parse("12.4").unwrap() < Version::parse("12.4.1").unwrap());
      }

      #[test]
      fn eq_ignores_raw_string_differences() {
          // 12.04 and 12.4 compare equal (numeric); raw is preserved separately.
          let a = Version::parse("12.04").unwrap();
          let b = Version::parse("12.4").unwrap();
          assert_eq!(a, b);
          assert_eq!(a.raw, "12.04");
      }

      #[test]
      fn display_renders_raw() {
          assert_eq!(Version::parse("12.4.1").unwrap().to_string(), "12.4.1");
      }
  }
  ```

- [ ] **Step:** Run it, see it fail (the module/type does not exist yet):

  ```bash
  cargo test -p cuvm-core version::
  ```

  **Expected:** fail — `error[E0433]: failed to resolve: use of undeclared crate or module \`version\`` (or `cannot find type \`Version\``). Compilation error, zero tests run.

- [ ] **Step:** Minimal implementation. Write the full `version.rs` (custom `Ord`/`PartialOrd`/`PartialEq`/`Eq` over `fields` only, padding the shorter with 0; `raw` kept verbatim; serde uses the raw string form so manifests round-trip cleanly):

  ```rust
  use std::cmp::Ordering;
  use std::fmt;

  use serde::{Deserialize, Deserializer, Serialize, Serializer};

  use crate::error::CoreError;

  /// A dotted version compared **field-by-field numerically**, missing tail = 0.
  /// `raw` preserves the original string for round-trip display.
  #[derive(Debug, Clone)]
  pub struct Version {
      pub fields: Vec<u32>,
      pub raw: String,
  }

  impl Version {
      pub fn parse(s: &str) -> Result<Self, CoreError> {
          let s = s.trim();
          if s.is_empty() {
              return Err(CoreError::InvalidVersion {
                  raw: s.to_string(),
              });
          }
          let mut fields = Vec::new();
          for part in s.split('.') {
              let n: u32 = part.parse().map_err(|_| CoreError::InvalidVersion {
                  raw: s.to_string(),
              })?;
              fields.push(n);
          }
          Ok(Version {
              fields,
              raw: s.to_string(),
          })
      }

      pub fn major(&self) -> u32 {
          self.fields.first().copied().unwrap_or(0)
      }
  }

  impl PartialEq for Version {
      fn eq(&self, other: &Self) -> bool {
          self.cmp(other) == Ordering::Equal
      }
  }
  impl Eq for Version {}

  impl PartialOrd for Version {
      fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
          Some(self.cmp(other))
      }
  }

  impl Ord for Version {
      fn cmp(&self, other: &Self) -> Ordering {
          let n = self.fields.len().max(other.fields.len());
          for i in 0..n {
              let a = self.fields.get(i).copied().unwrap_or(0);
              let b = other.fields.get(i).copied().unwrap_or(0);
              match a.cmp(&b) {
                  Ordering::Equal => continue,
                  non_eq => return non_eq,
              }
          }
          Ordering::Equal
      }
  }

  impl fmt::Display for Version {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
          f.write_str(&self.raw)
      }
  }

  impl Serialize for Version {
      fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
          s.serialize_str(&self.raw)
      }
  }

  impl<'de> Deserialize<'de> for Version {
      fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
          let raw = String::deserialize(d)?;
          Version::parse(&raw).map_err(serde::de::Error::custom)
      }
  }
  ```

  Then declare the modules + re-exports in `crates/cuvm-core/src/lib.rs` (replace WU-0's placeholder body):

  ```rust
  //! cuvm-core — pure domain types with ZERO I/O dependencies.

  pub mod domain;
  pub mod envplan;
  pub mod error;
  pub mod manifest;
  pub mod version;

  pub use domain::{
      Alias, Arch, Bundle, Companion, Cudnn, Driver, GpuClass, Os, Pin, Platform, Shell, Source,
      Toolkit,
  };
  pub use envplan::EnvPlan;
  pub use error::{CompatError, CoreError};
  pub use manifest::{BundleRecord, DriverRecord, Manifest, VersionMeta};
  pub use version::Version;
  ```

  > `lib.rs` references modules created in Tasks 1.3–1.5. To keep this step compiling on its own, create empty placeholder files first: `touch crates/cuvm-core/src/domain.rs crates/cuvm-core/src/envplan.rs crates/cuvm-core/src/error.rs crates/cuvm-core/src/manifest.rs` — but they will be empty, so the `pub use` lines would fail. Instead, do the next step: stub `error.rs` (needed by `version.rs`) now, and add the other `pub use` lines only as each module lands. For this task, set `lib.rs` to **only**:
  >
  > ```rust
  > //! cuvm-core — pure domain types with ZERO I/O dependencies.
  > pub mod error;
  > pub mod version;
  > pub use error::CoreError;
  > pub use version::Version;
  > ```
  >
  > and create a minimal `error.rs` containing only the `CoreError` variant `version.rs` uses:
  >
  > ```rust
  > use thiserror::Error;
  >
  > #[derive(Debug, Error)]
  > pub enum CoreError {
  >     #[error("invalid version string: {raw:?}")]
  >     InvalidVersion { raw: String },
  > }
  > ```
  >
  > The full `error.rs` and the remaining `pub mod`/`pub use` lines are completed in Task 1.6; that task replaces this minimal `lib.rs` with the full one shown above.

- [ ] **Step:** Run tests, see pass:

  ```bash
  cargo test -p cuvm-core version::
  ```

  **Expected:** `test result: ok. 7 passed; 0 failed`.

- [ ] **Step:** Commit.

  ```bash
  git add crates/cuvm-core/src/version.rs crates/cuvm-core/src/error.rs crates/cuvm-core/src/lib.rs
  git commit -m "feat(core): Version with numeric tuple ordering and parse

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 1.3 — Domain enums + structs (`domain.rs`)

**Files:**
- Create: `crates/cuvm-core/src/domain.rs`
- Modify: `crates/cuvm-core/src/lib.rs` (add `pub mod domain;` + re-exports)
- Test: inline `#[cfg(test)]` in `crates/cuvm-core/src/domain.rs`

- [ ] **Step:** Write the failing test. The load-bearing behaviors here are `Platform::redist_key()` (mirrors the redist platform dirs, spec §2.1) and `Bundle::handle()` (== `toolkit.version`, per CONTRACT). The structs are plain data, so the test exercises construction + the two methods + `Source` serde (used by `BundleRecord`):

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn redist_key_matches_redist_platform_dirs() {
          assert_eq!(
              Platform { os: Os::Linux, arch: Arch::X86_64 }.redist_key(),
              "linux-x86_64"
          );
          assert_eq!(
              Platform { os: Os::Linux, arch: Arch::Sbsa }.redist_key(),
              "linux-sbsa"
          );
          assert_eq!(
              Platform { os: Os::Linux, arch: Arch::Aarch64 }.redist_key(),
              "linux-aarch64"
          );
          assert_eq!(
              Platform { os: Os::Windows, arch: Arch::X86_64 }.redist_key(),
              "windows-x86_64"
          );
      }

      #[test]
      fn bundle_handle_equals_toolkit_version_raw() {
          let tk = Toolkit {
              version: crate::Version::parse("12.4.1").unwrap(),
              source: Source::Downloaded,
              root: std::path::PathBuf::from("/home/u/.cuvm/versions/12.4.1"),
              platform: Platform { os: Os::Linux, arch: Arch::X86_64 },
              components: vec!["cuda_nvcc".into(), "cuda_cudart".into()],
              has_lib64: false,
              installed_at: time::OffsetDateTime::UNIX_EPOCH,
              checksum: None,
          };
          let b = Bundle { toolkit: tk, cudnn: None, extra: vec![] };
          assert_eq!(b.handle(), "12.4.1");
      }

      #[test]
      fn source_serde_round_trips_lowercase() {
          let json = serde_json::to_string(&Source::Adopted).unwrap();
          assert_eq!(json, "\"adopted\"");
          let back: Source = serde_json::from_str("\"downloaded\"").unwrap();
          assert!(matches!(back, Source::Downloaded));
      }
  }
  ```

- [ ] **Step:** Run it, see it fail:

  ```bash
  cargo test -p cuvm-core domain::
  ```

  **Expected:** fail — `error[E0433]: ... undeclared ... module \`domain\`` / `cannot find type \`Platform\``. Compilation error, zero tests run.

- [ ] **Step:** Minimal implementation. Write the full `domain.rs` (every CONTRACT type; `Source` is `#[serde(rename_all = "lowercase")]` so `BundleRecord`/`VersionMeta` serialize it as the manifest expects):

  ```rust
  use std::path::PathBuf;

  use serde::{Deserialize, Serialize};
  use time::OffsetDateTime;

  use crate::Version;

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Os {
      Linux,
      Windows,
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Arch {
      X86_64,
      Sbsa,
      Aarch64,
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub struct Platform {
      pub os: Os,
      pub arch: Arch,
  }

  impl Platform {
      /// The redist platform-directory key, e.g. `"linux-x86_64"`.
      pub fn redist_key(&self) -> String {
          let os = match self.os {
              Os::Linux => "linux",
              Os::Windows => "windows",
          };
          let arch = match self.arch {
              Arch::X86_64 => "x86_64",
              Arch::Sbsa => "sbsa",
              Arch::Aarch64 => "aarch64",
          };
          format!("{os}-{arch}")
      }
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Shell {
      Bash,
      Zsh,
      PowerShell,
      Cmd,
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "lowercase")]
  pub enum Source {
      Adopted,
      Downloaded,
      Supplied,
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum GpuClass {
      Unknown,
      GeForce,
      DataCenter,
      Jetson,
      NgcReadyRtx,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Toolkit {
      pub version: Version,
      pub source: Source,
      pub root: PathBuf,
      pub platform: Platform,
      pub components: Vec<String>,
      pub has_lib64: bool,
      pub installed_at: OffsetDateTime,
      pub checksum: Option<String>,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Cudnn {
      pub version: Version,
      pub cuda_major: u32,
      pub source: Source,
      pub store: PathBuf,
      pub sha256: String,
      pub libs: Vec<String>,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Companion {
      pub name: String,
      pub version: Version,
      pub store: PathBuf,
      pub sha256: String,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Bundle {
      pub toolkit: Toolkit,
      pub cudnn: Option<Cudnn>,
      pub extra: Vec<Companion>,
  }

  impl Bundle {
      /// The stable handle for a bundle == the toolkit version's raw string.
      pub fn handle(&self) -> String {
          self.toolkit.version.raw.clone()
      }
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Alias {
      pub name: String,
      pub target: String,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Pin {
      pub spec: String,
      pub file: PathBuf,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Driver {
      pub present: bool,
      pub version: Version,
      pub platform: Platform,
      pub gpu_class: GpuClass,
  }
  ```

  Add to `crates/cuvm-core/src/lib.rs` (after the `version` module line):

  ```rust
  pub mod domain;
  pub use domain::{
      Alias, Arch, Bundle, Companion, Cudnn, Driver, GpuClass, Os, Pin, Platform, Shell, Source,
      Toolkit,
  };
  ```

- [ ] **Step:** Run tests, see pass:

  ```bash
  cargo test -p cuvm-core domain::
  ```

  **Expected:** `test result: ok. 3 passed; 0 failed`.

- [ ] **Step:** Commit.

  ```bash
  git add crates/cuvm-core/src/domain.rs crates/cuvm-core/src/lib.rs
  git commit -m "feat(core): domain enums and structs (Platform::redist_key, Bundle::handle)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 1.4 — Serde manifest value types + round-trip (`manifest.rs`)

**Files:**
- Create: `crates/cuvm-core/src/manifest.rs`
- Modify: `crates/cuvm-core/src/lib.rs` (add `pub mod manifest;` + re-exports)
- Test: inline `#[cfg(test)]` in `crates/cuvm-core/src/manifest.rs`

- [ ] **Step:** Write the failing test. The CONTRACT defines `Manifest`/`BundleRecord`/`VersionMeta`/`DriverRecord`. Spec §6 names `manifest.json` the source of truth, and §13 requires a "manifest round-trip" test. `aliases`/`pins` are `BTreeMap` (deterministic ordering for golden tests). `installed_at` is `OffsetDateTime` serialized via time's well-known RFC3339:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use std::collections::BTreeMap;
      use time::macros::datetime;

      fn sample() -> Manifest {
          let mut aliases = BTreeMap::new();
          aliases.insert("default".to_string(), "12.4.1".to_string());
          aliases.insert("lts".to_string(), "11.8.0".to_string());
          let mut pins = BTreeMap::new();
          pins.insert("/home/u/proj".to_string(), "12.4".to_string());
          Manifest {
              schema_version: 1,
              bundles: vec![BundleRecord {
                  version: "12.4.1".to_string(),
                  source: crate::Source::Downloaded,
                  path: "/home/u/.cuvm/versions/12.4.1".to_string(),
                  cudnn: Some("9.8.0".to_string()),
                  components: vec!["cuda_nvcc".into(), "cuda_cudart".into()],
                  sha256: Some("abc123".to_string()),
                  installed_at: datetime!(2026-06-08 10:30:00 UTC),
              }],
              aliases,
              pins,
              last_driver: Some(DriverRecord {
                  version: "550.54.14".to_string(),
                  cuda_ceiling: "12.4".to_string(),
              }),
          }
      }

      #[test]
      fn manifest_round_trips_through_json() {
          let m = sample();
          let json = serde_json::to_string_pretty(&m).unwrap();
          let back: Manifest = serde_json::from_str(&json).unwrap();
          assert_eq!(m, back);
      }

      #[test]
      fn manifest_json_uses_expected_field_names() {
          let m = sample();
          let json = serde_json::to_string(&m).unwrap();
          assert!(json.contains("\"schema_version\":1"));
          assert!(json.contains("\"last_driver\""));
          assert!(json.contains("\"cuda_ceiling\":\"12.4\""));
          // Source serialized lowercase via domain::Source.
          assert!(json.contains("\"source\":\"downloaded\""));
      }

      #[test]
      fn aliases_serialize_in_btreemap_sorted_order() {
          let json = serde_json::to_string(&sample()).unwrap();
          // "default" sorts before "lts".
          let d = json.find("\"default\"").unwrap();
          let l = json.find("\"lts\"").unwrap();
          assert!(d < l, "BTreeMap must emit aliases sorted for golden stability");
      }

      #[test]
      fn version_meta_round_trips() {
          let vm = VersionMeta {
              version: "13.3.0".to_string(),
              source: crate::Source::Downloaded,
              cudnn: None,
              components: vec!["cuda_nvcc".into(), "cuda_crt".into(), "cccl".into()],
              sha256: None,
              has_lib64: false,
              installed_at: datetime!(2026-06-08 11:00:00 UTC),
          };
          let json = serde_json::to_string(&vm).unwrap();
          let back: VersionMeta = serde_json::from_str(&json).unwrap();
          assert_eq!(vm, back);
          assert!(json.contains("\"has_lib64\":false"));
      }
  }
  ```

- [ ] **Step:** Run it, see it fail:

  ```bash
  cargo test -p cuvm-core manifest::
  ```

  **Expected:** fail — `error[E0433]: ... undeclared ... module \`manifest\``. Compilation error, zero tests run.

- [ ] **Step:** Minimal implementation. Write the full `manifest.rs`. `OffsetDateTime` needs the `time::serde::rfc3339` adapter (the `serde`+`formatting`+`parsing` features were added in Task 1.1):

  ```rust
  use std::collections::BTreeMap;

  use serde::{Deserialize, Serialize};
  use time::OffsetDateTime;

  use crate::Source;

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct Manifest {
      pub schema_version: u32,
      pub bundles: Vec<BundleRecord>,
      pub aliases: BTreeMap<String, String>,
      pub pins: BTreeMap<String, String>,
      #[serde(skip_serializing_if = "Option::is_none", default)]
      pub last_driver: Option<DriverRecord>,
  }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct BundleRecord {
      pub version: String,
      pub source: Source,
      pub path: String,
      #[serde(skip_serializing_if = "Option::is_none", default)]
      pub cudnn: Option<String>,
      pub components: Vec<String>,
      #[serde(skip_serializing_if = "Option::is_none", default)]
      pub sha256: Option<String>,
      #[serde(with = "time::serde::rfc3339")]
      pub installed_at: OffsetDateTime,
  }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct VersionMeta {
      pub version: String,
      pub source: Source,
      #[serde(skip_serializing_if = "Option::is_none", default)]
      pub cudnn: Option<String>,
      pub components: Vec<String>,
      #[serde(skip_serializing_if = "Option::is_none", default)]
      pub sha256: Option<String>,
      pub has_lib64: bool,
      #[serde(with = "time::serde::rfc3339")]
      pub installed_at: OffsetDateTime,
  }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct DriverRecord {
      pub version: String,
      pub cuda_ceiling: String,
  }
  ```

  Add to `crates/cuvm-core/src/lib.rs`:

  ```rust
  pub mod manifest;
  pub use manifest::{BundleRecord, DriverRecord, Manifest, VersionMeta};
  ```

- [ ] **Step:** Run tests, see pass:

  ```bash
  cargo test -p cuvm-core manifest::
  ```

  **Expected:** `test result: ok. 4 passed; 0 failed`.

- [ ] **Step:** Commit.

  ```bash
  git add crates/cuvm-core/src/manifest.rs crates/cuvm-core/src/lib.rs
  git commit -m "feat(core): serde manifest value types with JSON round-trip

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 1.5 — `EnvPlan` (OS-neutral activation intermediate)

**Files:**
- Create: `crates/cuvm-core/src/envplan.rs`
- Modify: `crates/cuvm-core/src/lib.rs` (add `pub mod envplan;` + re-export)
- Test: inline `#[cfg(test)]` in `crates/cuvm-core/src/envplan.rs`

- [ ] **Step:** Write the failing test. Per spec §5, `EnvPlan` is "the intermediate the Activator renders per shell — makes golden-file tests trivial". In WU-1 it is a plain value type (the renderer lands in WU-5/WU-9); the test asserts the CONTRACT field shape and that it is `Clone`/`PartialEq` (needed by later golden tests). Fields mirror the bash emission in spec §8:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn envplan_holds_the_activation_fields() {
          let p = EnvPlan {
              cuda_home: "/home/u/.cuvm/versions/12.4.1".into(),
              cuda_path: "/home/u/.cuvm/versions/12.4.1".into(),
              toolkit_root: "/home/u/.cuvm/versions/12.4.1".into(),
              prepend_path: vec!["/home/u/.cuvm/versions/12.4.1/bin".into()],
              prepend_lib: vec!["/home/u/.cuvm/versions/12.4.1/lib64".into()],
              current: "12.4.1".into(),
              injected: vec![
                  "/home/u/.cuvm/versions/12.4.1/bin".into(),
                  "/home/u/.cuvm/versions/12.4.1/lib64".into(),
              ],
          };
          assert_eq!(p.cuda_home, p.cuda_path);
          assert_eq!(p.cuda_path, p.toolkit_root);
          assert_eq!(p.prepend_path.len(), 1);
          assert_eq!(p.injected.len(), 2);
          assert_eq!(p.current, "12.4.1");
          // Clone + PartialEq are part of the contract for golden tests.
          assert_eq!(p.clone(), p);
      }
  }
  ```

- [ ] **Step:** Run it, see it fail:

  ```bash
  cargo test -p cuvm-core envplan::
  ```

  **Expected:** fail — `error[E0433]: ... undeclared ... module \`envplan\``. Compilation error, zero tests run.

- [ ] **Step:** Minimal implementation. Write `envplan.rs`:

  ```rust
  /// OS-neutral activation intermediate; an `Activator` renders this per `Shell`.
  /// Fields mirror the env-script contract in the spec (§8).
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct EnvPlan {
      pub cuda_home: String,
      pub cuda_path: String,
      pub toolkit_root: String,
      pub prepend_path: Vec<String>,
      pub prepend_lib: Vec<String>,
      pub current: String,
      pub injected: Vec<String>,
  }
  ```

  Add to `crates/cuvm-core/src/lib.rs`:

  ```rust
  pub mod envplan;
  pub use envplan::EnvPlan;
  ```

- [ ] **Step:** Run tests, see pass:

  ```bash
  cargo test -p cuvm-core envplan::
  ```

  **Expected:** `test result: ok. 1 passed; 0 failed`.

- [ ] **Step:** Commit.

  ```bash
  git add crates/cuvm-core/src/envplan.rs crates/cuvm-core/src/lib.rs
  git commit -m "feat(core): EnvPlan activation intermediate

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 1.6 — Finalize `error.rs` (`CoreError` + `CompatError`) and full `lib.rs`

**Files:**
- Modify: `crates/cuvm-core/src/error.rs` (expand from the Task-1.2 minimal stub)
- Modify: `crates/cuvm-core/src/lib.rs` (final, complete form)
- Test: inline `#[cfg(test)]` in `crates/cuvm-core/src/error.rs`

- [ ] **Step:** Write the failing test. The CONTRACT names `CompatErr/CoreErr via thiserror: NotInstalled, DriverCeiling, CudnnMismatch, ...`. Assert the variants exist and their `Display` (thiserror `#[error(...)]`) renders the load-bearing facts (spec §2.4: driver ceiling, cuDNN-major mismatch):

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::Version;

      #[test]
      fn core_error_invalid_version_displays_raw() {
          let e = CoreError::InvalidVersion { raw: "12.x".into() };
          assert!(e.to_string().contains("12.x"));
      }

      #[test]
      fn compat_error_not_installed_names_the_spec() {
          let e = CompatError::NotInstalled { spec: "12.4".into() };
          let msg = e.to_string();
          assert!(msg.contains("12.4"));
          assert!(msg.to_lowercase().contains("not installed"));
      }

      #[test]
      fn compat_error_driver_ceiling_reports_both_versions() {
          let e = CompatError::DriverCeiling {
              want: Version::parse("13.0").unwrap(),
              ceiling: Version::parse("12.4").unwrap(),
          };
          let msg = e.to_string();
          assert!(msg.contains("13.0"));
          assert!(msg.contains("12.4"));
      }

      #[test]
      fn compat_error_cudnn_mismatch_reports_majors() {
          let e = CompatError::CudnnMismatch {
              cuda_major: 13,
              cudnn_major: 8,
          };
          let msg = e.to_string();
          assert!(msg.contains("13"));
          assert!(msg.contains('8'));
      }
  }
  ```

- [ ] **Step:** Run it, see it fail (only `InvalidVersion` exists from the Task-1.2 stub):

  ```bash
  cargo test -p cuvm-core error::
  ```

  **Expected:** fail — `error[E0599]: no variant ... \`NotInstalled\` ... for enum \`CompatError\`` (and `CompatError` itself undefined). Compilation error.

- [ ] **Step:** Minimal implementation. Replace `crates/cuvm-core/src/error.rs` with the full version (keep `CoreError::InvalidVersion` that `version.rs` depends on; add `CompatError` with the contract variants):

  ```rust
  use thiserror::Error;

  use crate::Version;

  /// Errors from pure-core parsing / construction.
  #[derive(Debug, Error)]
  pub enum CoreError {
      #[error("invalid version string: {raw:?}")]
      InvalidVersion { raw: String },
  }

  /// Compatibility / resolution decisions surfaced by the compat engine and resolver.
  #[derive(Debug, Error)]
  pub enum CompatError {
      #[error("no toolkit matching {spec:?} is installed (not installed)")]
      NotInstalled { spec: String },

      #[error("requested CUDA {want} exceeds the driver ceiling {ceiling}")]
      DriverCeiling { want: Version, ceiling: Version },

      #[error("cuDNN major {cudnn_major} is incompatible with CUDA major {cuda_major}")]
      CudnnMismatch { cuda_major: u32, cudnn_major: u32 },
  }
  ```

  Then set `crates/cuvm-core/src/lib.rs` to its final, complete form (this replaces the minimal `lib.rs` from Task 1.2 and consolidates every module + re-export):

  ```rust
  //! cuvm-core — pure domain types with ZERO I/O dependencies.
  //!
  //! No http, no fs, no async in the public API: just numeric versions, domain
  //! structs, serde manifest value types, the OS-neutral `EnvPlan`, and errors.

  pub mod domain;
  pub mod envplan;
  pub mod error;
  pub mod manifest;
  pub mod version;

  pub use domain::{
      Alias, Arch, Bundle, Companion, Cudnn, Driver, GpuClass, Os, Pin, Platform, Shell, Source,
      Toolkit,
  };
  pub use envplan::EnvPlan;
  pub use error::{CompatError, CoreError};
  pub use manifest::{BundleRecord, DriverRecord, Manifest, VersionMeta};
  pub use version::Version;
  ```

- [ ] **Step:** Run the full core test suite, see pass (all modules together):

  ```bash
  cargo test -p cuvm-core
  ```

  **Expected:** `test result: ok. 19 passed; 0 failed` (7 version + 3 domain + 4 manifest + 1 envplan + 4 error).

- [ ] **Step:** Commit.

  ```bash
  git add crates/cuvm-core/src/error.rs crates/cuvm-core/src/lib.rs
  git commit -m "feat(core): CoreError + CompatError variants and finalize lib re-exports

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 1.7 — Supporting port input/output types in `cuvm-app`

**Files:**
- Create: `crates/cuvm-app/src/ports.rs` (types portion first)
- Modify: `crates/cuvm-app/src/lib.rs`
- Test: inline `#[cfg(test)]` in `crates/cuvm-app/src/ports.rs`

> The trait ports reference value types that are *not* in `cuvm-core` because they are app-layer use-case shapes: `Resolved`, `ResolveVia`, `Verdict`, `Severity`, `Artifact`, plus opaque-for-now `AcquirePlan`, `Cached`, `VersionMeta` is in core, `ArtifactKind`, `Candidate`, `ComponentPolicy`. We define them here so the traits compile. Real fields beyond the CONTRACT-specified ones are filled in their owning WUs; for now give each only the fields the CONTRACT pins down, and an empty placeholder where the CONTRACT is silent (documented as "expanded in WU-N").

- [ ] **Step:** Write the failing test. Assert the CONTRACT-pinned shapes: `Resolved { bundle, spec, via, pin }`, `ResolveVia` variants, `Verdict { ok, severity, reason, forward_compat_possible }`, `Severity`, `Artifact { component, relative_path, url, sha256, md5, size }`:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use cuvm_core::{Arch, Bundle, Os, Platform, Source, Toolkit, Version};

      fn bundle() -> Bundle {
          Bundle {
              toolkit: Toolkit {
                  version: Version::parse("12.4.1").unwrap(),
                  source: Source::Downloaded,
                  root: "/p".into(),
                  platform: Platform { os: Os::Linux, arch: Arch::X86_64 },
                  components: vec![],
                  has_lib64: false,
                  installed_at: time::OffsetDateTime::UNIX_EPOCH,
                  checksum: None,
              },
              cudnn: None,
              extra: vec![],
          }
      }

      #[test]
      fn resolved_carries_bundle_spec_via_and_pin() {
          let r = Resolved {
              bundle: bundle(),
              spec: "12.4".to_string(),
              via: ResolveVia::Minor,
              pin: None,
          };
          assert_eq!(r.spec, "12.4");
          assert!(matches!(r.via, ResolveVia::Minor));
          assert!(r.pin.is_none());
      }

      #[test]
      fn verdict_blocks_with_reason() {
          let v = Verdict {
              ok: false,
              severity: Severity::Block,
              reason: "driver ceiling exceeded".to_string(),
              forward_compat_possible: false,
          };
          assert!(!v.ok);
          assert!(matches!(v.severity, Severity::Block));
          assert_eq!(v.reason, "driver ceiling exceeded");
      }

      #[test]
      fn artifact_mirrors_one_redist_platform_object() {
          let a = Artifact {
              component: "cuda_nvcc".to_string(),
              relative_path: "cuda_nvcc/linux-x86_64/cuda_nvcc-linux-x86_64-12.4.131-archive.tar.xz"
                  .to_string(),
              url: "https://developer.download.nvidia.com/compute/cuda/redist/...".to_string(),
              sha256: "deadbeef".to_string(),
              md5: None,
              size: 1234,
          };
          assert_eq!(a.component, "cuda_nvcc");
          assert!(a.relative_path.starts_with("cuda_nvcc/"));
          assert_eq!(a.size, 1234);
      }
  }
  ```

- [ ] **Step:** Run it, see it fail:

  ```bash
  cargo test -p cuvm-app ports::
  ```

  **Expected:** fail — `error[E0432]: unresolved import ... \`ports\`` / `cannot find type \`Resolved\``. Compilation error, zero tests run.

- [ ] **Step:** Minimal implementation. Create `crates/cuvm-app/src/ports.rs` with the value types (traits added in Task 1.8). Use `cuvm_core` types where they exist:

  ```rust
  use std::path::PathBuf;

  use cuvm_core::{Bundle, Pin};

  // ----- Resolver outputs -----

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Resolved {
      pub bundle: Bundle,
      pub spec: String,
      pub via: ResolveVia,
      pub pin: Option<Pin>,
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum ResolveVia {
      Exact,
      Minor,
      Major,
      Latest,
      Alias,
      PinFile,
      Default,
  }

  // ----- Compat engine outputs -----

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Verdict {
      pub ok: bool,
      pub severity: Severity,
      pub reason: String,
      pub forward_compat_possible: bool,
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Severity {
      Ok,
      Warn,
      Block,
  }

  // ----- Registry outputs -----

  /// Mirrors one redist platform object; `relative_path` is taken verbatim.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Artifact {
      pub component: String,
      pub relative_path: String,
      pub url: String,
      pub sha256: String,
      pub md5: Option<String>,
      pub size: u64,
  }

  // ----- Installer inputs/outputs (fields expanded in their owning WUs) -----

  /// What to acquire for an install. Fields land in WU-10/WU-13/WU-14.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct AcquirePlan {
      pub artifacts: Vec<Artifact>,
      pub dest_handle: String,
  }

  /// A downloaded, on-disk artifact. Fields land in WU-11/WU-12.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Cached {
      pub artifact: Artifact,
      pub path: PathBuf,
  }

  /// Kind of user-supplied artifact for `ingest_supplied`. Expanded in WU-17.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum ArtifactKind {
      Toolkit,
      Cudnn,
  }

  /// A scan candidate (existing on-disk install). Fields land in WU-4/WU-9.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Candidate {
      pub root: PathBuf,
      pub version_hint: Option<String>,
  }

  /// Which components to request from the registry. Expanded in WU-10.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum ComponentPolicy {
      /// The minimal usable set (manifest-driven, version-branched).
      Recommended,
      /// An explicit component-name allowlist.
      Only(Vec<String>),
  }
  ```

  Set `crates/cuvm-app/src/lib.rs` (this WU's app surface):

  ```rust
  //! cuvm-app — use-cases and trait ports. Depends only on `cuvm-core`.

  pub mod ports;

  pub use ports::{
      AcquirePlan, Artifact, ArtifactKind, Cached, Candidate, ComponentPolicy, Resolved, ResolveVia,
      Severity, Verdict,
  };
  ```

- [ ] **Step:** Run tests, see pass:

  ```bash
  cargo test -p cuvm-app ports::
  ```

  **Expected:** `test result: ok. 3 passed; 0 failed`.

- [ ] **Step:** Commit.

  ```bash
  git add crates/cuvm-app/src/ports.rs crates/cuvm-app/src/lib.rs
  git commit -m "feat(app): port input/output value types (Resolved, Verdict, Artifact, ...)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 1.8 — Trait ports in `cuvm-app` (object-safe; mockable)

**Files:**
- Modify: `crates/cuvm-app/src/ports.rs` (add the seven traits)
- Test: inline `#[cfg(test)]` in `crates/cuvm-app/src/ports.rs`

> Every trait must be **object-safe** (used behind `Box<dyn ..>` in `cuvm-platform`/`cuvm-cli`) and async-free, returning `anyhow::Result`. The CONTRACT lists `Inventory` with `list/deregister/set_alias/load/save`; the spec §5 also shows `scan/adopt` on `Inventory` while the CONTRACT puts `scan/adopt` on `Installer`. The CONTRACT (SHARED CONTRACT, authoritative for composition) is followed: `scan/adopt` live on **`Installer`**.

- [ ] **Step:** Write the failing test. The decisive WU-1 behavior is *object safety*: a `fn` that takes `&dyn Trait` for each port must compile. Add this to the `tests` module in `ports.rs`:

  ```rust
      // Object-safety witnesses: each must accept a trait object.
      // (If any trait were not object-safe, these fns would fail to compile.)
      fn _assert_resolver_object_safe(_: &dyn Resolver) {}
      fn _assert_activator_object_safe(_: &dyn Activator) {}
      fn _assert_installer_object_safe(_: &dyn Installer) {}
      fn _assert_inventory_object_safe(_: &dyn Inventory) {}
      fn _assert_registry_object_safe(_: &dyn RegistryClient) {}
      fn _assert_driverprobe_object_safe(_: &dyn DriverProbe) {}
      fn _assert_compat_object_safe(_: &dyn CompatEngine) {}

      #[test]
      fn ports_are_object_safe() {
          // Compiling the witnesses above is the assertion; this test just anchors them.
          fn takes_fn(_f: fn(&dyn Resolver)) {}
          takes_fn(_assert_resolver_object_safe);
      }
  ```

- [ ] **Step:** Run it, see it fail (traits do not exist yet):

  ```bash
  cargo test -p cuvm-app ports::ports_are_object_safe
  ```

  **Expected:** fail — `error[E0405]: cannot find trait \`Resolver\` in this scope` (and the other six). Compilation error.

- [ ] **Step:** Minimal implementation. Append the seven traits to `ports.rs` (signatures verbatim from the CONTRACT; `anyhow::Result` for the fallible ones; `CompatEngine::check_toolkit`/`validate_pair` return `Verdict` directly and `pair_cudnn` returns `Option<Version>`, per CONTRACT):

  ```rust
  use std::path::Path;

  use anyhow::Result;
  use cuvm_core::{Driver, Manifest, Platform, Shell, Version, VersionMeta};

  pub trait Resolver {
      fn resolve(&self, spec: &str) -> Result<Resolved>;
      fn resolve_from_dir(&self, cwd: &Path) -> Result<Option<Resolved>>;
      fn expand_alias(&self, name: &str) -> Result<String>;
      fn find_pin_upward(&self, cwd: &Path) -> Result<Option<cuvm_core::Pin>>;
  }

  pub trait Activator {
      fn emit_env(&self, b: &Bundle, sh: Shell) -> Result<String>;
      fn emit_deactivate(&self, sh: Shell) -> Result<String>;
      fn hook(&self, sh: Shell) -> Result<String>;
      fn supports(&self, sh: Shell) -> bool;
  }

  pub trait Installer {
      fn acquire(&self, plan: &AcquirePlan) -> Result<Vec<Cached>>;
      fn verify(&self, arts: &[Cached]) -> Result<()>;
      fn extract_atomic(&self, arts: &[Cached], tmp: &Path) -> Result<std::path::PathBuf>;
      fn place(&self, tmp: &Path, dst: &Path, meta: &VersionMeta) -> Result<()>;
      fn smoke_test(&self, root: &Path) -> Result<()>;
      fn ingest_supplied(&self, file: &Path, kind: ArtifactKind) -> Result<std::path::PathBuf>;
      fn scan(&self) -> Result<Vec<Candidate>>;
      fn adopt(&self, c: &Candidate) -> Result<Bundle>;
  }

  pub trait Inventory {
      fn list(&self) -> Result<Vec<Bundle>>;
      fn deregister(&self, handle: &str) -> Result<()>;
      fn set_alias(&self, n: &str, t: &str) -> Result<()>;
      fn load(&self) -> Result<Manifest>;
      fn save(&self, m: &Manifest) -> Result<()>;
  }

  pub trait RegistryClient {
      fn list_toolkits(&self, p: &Platform) -> Result<Vec<Version>>;
      fn list_cudnn(&self, p: &Platform, major: u32) -> Result<Vec<Version>>;
      fn resolve_toolkit(
          &self,
          v: &Version,
          p: &Platform,
          want: &ComponentPolicy,
      ) -> Result<Vec<Artifact>>;
      fn resolve_cudnn(&self, v: &Version, p: &Platform, major: u32) -> Result<Vec<Artifact>>;
  }

  pub trait DriverProbe {
      fn probe(&self) -> Result<Driver>;
  }

  pub trait CompatEngine {
      fn max_toolkit_for_driver(&self, d: &Driver) -> Result<Version>;
      fn check_toolkit(&self, d: &Driver, want: &Version, strict: bool) -> Verdict;
      fn pair_cudnn(&self, tk: &Version, avail: &[Version]) -> Option<Version>;
      fn validate_pair(&self, tk: &Version, cudnn: &Version) -> Verdict;
  }
  ```

  Update `crates/cuvm-app/src/lib.rs` re-exports to include the traits:

  ```rust
  pub use ports::{
      Activator, AcquirePlan, Artifact, ArtifactKind, Cached, Candidate, CompatEngine,
      ComponentPolicy, DriverProbe, Installer, Inventory, RegistryClient, Resolved, ResolveVia,
      Resolver, Severity, Verdict,
  };
  ```

  > Note `ports.rs` now needs `Bundle` in scope; it is already imported at the top via `use cuvm_core::{Bundle, Pin};` from Task 1.7. The new `use cuvm_core::{Driver, Manifest, Platform, Shell, Version, VersionMeta};` line adds the rest.

- [ ] **Step:** Run tests, see pass:

  ```bash
  cargo test -p cuvm-app
  ```

  **Expected:** `test result: ok. 4 passed; 0 failed` (3 type tests + object-safety anchor). The object-safety witnesses compile, proving every port is `dyn`-usable.

- [ ] **Step:** Commit.

  ```bash
  git add crates/cuvm-app/src/ports.rs crates/cuvm-app/src/lib.rs
  git commit -m "feat(app): object-safe trait ports (Resolver, Activator, Installer, ...)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 1.9 — Platform stubs: unix/windows Activator + Installer returning `NotImplemented`

**Files:**
- Create: `crates/cuvm-platform/src/unix.rs`
- Create: `crates/cuvm-platform/src/windows.rs`
- Modify: `crates/cuvm-platform/src/lib.rs` (declare modules; factory added in Task 1.10)
- Test: inline `#[cfg(test)]` in `crates/cuvm-platform/src/unix.rs` and `.../windows.rs`

> Both `unix.rs` and `windows.rs` are compiled on **every** host (plain `pub mod`, not `#[cfg]`-gated) — WU-1 has no syscalls, so Windows golden/dispatch tests run on Linux CI. The `#[cfg]` syscall floor lands in WU-5/WU-9. Each stub method returns an `anyhow::Error` carrying a stable `NotImplemented` marker so callers (and the WU-1 tests) can assert on it.

- [ ] **Step:** Write the failing test for the unix backend. Assert each `Activator`/`Installer` method returns `Err` whose message contains `not implemented` (the WU-1 stub contract). Put at the bottom of `unix.rs`:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use cuvm_app::{Activator, Installer};
      use cuvm_core::Shell;

      #[test]
      fn unix_activator_methods_are_not_implemented() {
          let a = UnixActivator::new();
          // supports() answers without I/O even in the stub (no panic, returns a bool).
          let _ = a.supports(Shell::Bash);
          let err = a.emit_deactivate(Shell::Bash).unwrap_err();
          assert!(err.to_string().to_lowercase().contains("not implemented"));
          let err = a.hook(Shell::Zsh).unwrap_err();
          assert!(err.to_string().to_lowercase().contains("not implemented"));
      }

      #[test]
      fn unix_installer_methods_are_not_implemented() {
          let i = UnixInstaller::new();
          let err = i.scan().unwrap_err();
          assert!(err.to_string().to_lowercase().contains("not implemented"));
          let err = i.smoke_test(std::path::Path::new("/nope")).unwrap_err();
          assert!(err.to_string().to_lowercase().contains("not implemented"));
      }
  }
  ```

- [ ] **Step:** Run it, see it fail (module/types absent):

  ```bash
  cargo test -p cuvm-platform unix::
  ```

  **Expected:** fail — `error[E0433]: ... undeclared ... module \`unix\`` / `cannot find ... \`UnixActivator\``. Compilation error.

- [ ] **Step:** Minimal implementation. Add a shared `not_impl()` helper + the unix stubs. Write `crates/cuvm-platform/src/unix.rs`:

  ```rust
  use std::path::{Path, PathBuf};

  use anyhow::Result;
  use cuvm_app::{
      AcquirePlan, Activator, ArtifactKind, Bundle, Cached, Candidate, Installer,
  };
  use cuvm_core::{Shell, VersionMeta};

  use crate::not_impl;

  /// Unix (`#[cfg(unix)]` syscalls land in WU-5/WU-13) Activator. WU-1 = stub.
  #[derive(Debug, Default)]
  pub struct UnixActivator;

  impl UnixActivator {
      pub fn new() -> Self {
          UnixActivator
      }
  }

  impl Activator for UnixActivator {
      fn emit_env(&self, _b: &Bundle, _sh: Shell) -> Result<String> {
          Err(not_impl("UnixActivator::emit_env"))
      }
      fn emit_deactivate(&self, _sh: Shell) -> Result<String> {
          Err(not_impl("UnixActivator::emit_deactivate"))
      }
      fn hook(&self, _sh: Shell) -> Result<String> {
          Err(not_impl("UnixActivator::hook"))
      }
      fn supports(&self, sh: Shell) -> bool {
          // Stub answer (no I/O): the unix backend will support bash/zsh in WU-5.
          matches!(sh, Shell::Bash | Shell::Zsh)
      }
  }

  /// Unix Installer. WU-1 = stub.
  #[derive(Debug, Default)]
  pub struct UnixInstaller;

  impl UnixInstaller {
      pub fn new() -> Self {
          UnixInstaller
      }
  }

  impl Installer for UnixInstaller {
      fn acquire(&self, _plan: &AcquirePlan) -> Result<Vec<Cached>> {
          Err(not_impl("UnixInstaller::acquire"))
      }
      fn verify(&self, _arts: &[Cached]) -> Result<()> {
          Err(not_impl("UnixInstaller::verify"))
      }
      fn extract_atomic(&self, _arts: &[Cached], _tmp: &Path) -> Result<PathBuf> {
          Err(not_impl("UnixInstaller::extract_atomic"))
      }
      fn place(&self, _tmp: &Path, _dst: &Path, _meta: &VersionMeta) -> Result<()> {
          Err(not_impl("UnixInstaller::place"))
      }
      fn smoke_test(&self, _root: &Path) -> Result<()> {
          Err(not_impl("UnixInstaller::smoke_test"))
      }
      fn ingest_supplied(&self, _file: &Path, _kind: ArtifactKind) -> Result<PathBuf> {
          Err(not_impl("UnixInstaller::ingest_supplied"))
      }
      fn scan(&self) -> Result<Vec<Candidate>> {
          Err(not_impl("UnixInstaller::scan"))
      }
      fn adopt(&self, _c: &Candidate) -> Result<Bundle> {
          Err(not_impl("UnixInstaller::adopt"))
      }
  }
  ```

  > `Bundle` must be re-exported from `cuvm-app` for these `use` paths. It is a `cuvm-core` type; add `pub use cuvm_core::Bundle;` to `cuvm-app/src/ports.rs`'s top-level re-exports if not already present — simpler: import `cuvm_core::Bundle` directly. To keep the stub imports clean, change the unix/windows `use` to import `Bundle` from `cuvm_core`: replace `cuvm_app::{... Bundle ...}` with `cuvm_app::{AcquirePlan, Activator, ArtifactKind, Cached, Candidate, Installer}` and add `use cuvm_core::{Bundle, Shell, VersionMeta};`.

  Apply that import correction now — final `use` block at the top of `unix.rs`:

  ```rust
  use std::path::{Path, PathBuf};

  use anyhow::Result;
  use cuvm_app::{AcquirePlan, Activator, ArtifactKind, Cached, Candidate, Installer};
  use cuvm_core::{Bundle, Shell, VersionMeta};

  use crate::not_impl;
  ```

  Set `crates/cuvm-platform/src/lib.rs` to declare the helper + modules (factory comes in Task 1.10):

  ```rust
  //! cuvm-platform — per-OS Activator/Installer backends behind a runtime factory.
  //!
  //! WU-1: stub backends returning `NotImplemented`. Real syscalls (registry,
  //! junction, broadcast, symlink) arrive behind `#[cfg]` in WU-5/WU-9/WU-13/WU-14.

  pub mod unix;
  pub mod windows;

  /// Stable "not implemented yet" error for WU-1 stubs.
  pub(crate) fn not_impl(what: &str) -> anyhow::Error {
      anyhow::anyhow!("{what}: not implemented (WU-1 stub)")
  }
  ```

- [ ] **Step:** Run the unix tests, see pass:

  ```bash
  cargo test -p cuvm-platform unix::
  ```

  **Expected:** `test result: ok. 2 passed; 0 failed`.

- [ ] **Step:** Now write the windows stub test + implementation mirroring unix. Put this test at the bottom of `crates/cuvm-platform/src/windows.rs`:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use cuvm_app::{Activator, Installer};
      use cuvm_core::Shell;

      #[test]
      fn windows_activator_methods_are_not_implemented() {
          let a = WindowsActivator::new();
          let _ = a.supports(Shell::PowerShell);
          let err = a.emit_deactivate(Shell::PowerShell).unwrap_err();
          assert!(err.to_string().to_lowercase().contains("not implemented"));
      }

      #[test]
      fn windows_installer_methods_are_not_implemented() {
          let i = WindowsInstaller::new();
          let err = i.scan().unwrap_err();
          assert!(err.to_string().to_lowercase().contains("not implemented"));
      }
  }
  ```

  And the implementation above it in `windows.rs`:

  ```rust
  use std::path::{Path, PathBuf};

  use anyhow::Result;
  use cuvm_app::{AcquirePlan, Activator, ArtifactKind, Cached, Candidate, Installer};
  use cuvm_core::{Bundle, Shell, VersionMeta};

  use crate::not_impl;

  /// Windows Activator (HKCU R-M-W + junction land in WU-9). WU-1 = stub.
  #[derive(Debug, Default)]
  pub struct WindowsActivator;

  impl WindowsActivator {
      pub fn new() -> Self {
          WindowsActivator
      }
  }

  impl Activator for WindowsActivator {
      fn emit_env(&self, _b: &Bundle, _sh: Shell) -> Result<String> {
          Err(not_impl("WindowsActivator::emit_env"))
      }
      fn emit_deactivate(&self, _sh: Shell) -> Result<String> {
          Err(not_impl("WindowsActivator::emit_deactivate"))
      }
      fn hook(&self, _sh: Shell) -> Result<String> {
          Err(not_impl("WindowsActivator::hook"))
      }
      fn supports(&self, sh: Shell) -> bool {
          // cmd is a degraded shell (no reliable cd-hook); powershell is primary.
          matches!(sh, Shell::PowerShell | Shell::Cmd)
      }
  }

  /// Windows Installer (redist `.zip` merge lands in WU-14). WU-1 = stub.
  #[derive(Debug, Default)]
  pub struct WindowsInstaller;

  impl WindowsInstaller {
      pub fn new() -> Self {
          WindowsInstaller
      }
  }

  impl Installer for WindowsInstaller {
      fn acquire(&self, _plan: &AcquirePlan) -> Result<Vec<Cached>> {
          Err(not_impl("WindowsInstaller::acquire"))
      }
      fn verify(&self, _arts: &[Cached]) -> Result<()> {
          Err(not_impl("WindowsInstaller::verify"))
      }
      fn extract_atomic(&self, _arts: &[Cached], _tmp: &Path) -> Result<PathBuf> {
          Err(not_impl("WindowsInstaller::extract_atomic"))
      }
      fn place(&self, _tmp: &Path, _dst: &Path, _meta: &VersionMeta) -> Result<()> {
          Err(not_impl("WindowsInstaller::place"))
      }
      fn smoke_test(&self, _root: &Path) -> Result<()> {
          Err(not_impl("WindowsInstaller::smoke_test"))
      }
      fn ingest_supplied(&self, _file: &Path, _kind: ArtifactKind) -> Result<PathBuf> {
          Err(not_impl("WindowsInstaller::ingest_supplied"))
      }
      fn scan(&self) -> Result<Vec<Candidate>> {
          Err(not_impl("WindowsInstaller::scan"))
      }
      fn adopt(&self, _c: &Candidate) -> Result<Bundle> {
          Err(not_impl("WindowsInstaller::adopt"))
      }
  }
  ```

- [ ] **Step:** Run all platform tests, see pass:

  ```bash
  cargo test -p cuvm-platform
  ```

  **Expected:** `test result: ok. 4 passed; 0 failed` (2 unix + 2 windows).

- [ ] **Step:** Commit.

  ```bash
  git add crates/cuvm-platform/src/unix.rs crates/cuvm-platform/src/windows.rs crates/cuvm-platform/src/lib.rs
  git commit -m "feat(platform): unix/windows Activator+Installer stubs returning NotImplemented

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 1.10 — Runtime factory `new_activator`/`new_installer` (the seam)

**Files:**
- Modify: `crates/cuvm-platform/src/lib.rs` (add factory fns + trait-object compliance tests)
- Test: inline `#[cfg(test)]` in `crates/cuvm-platform/src/lib.rs`

> This is the WU-1 deliverable that enables parallel Linux/Windows tracks. The factory dispatches on the **`Os` value at runtime** (not `#[cfg]`), so both backends are reachable on any host and the table test below runs on Linux CI.

- [ ] **Step:** Write the failing test. Cover the two CONTRACT requirements: (a) trait-object compliance — `let _: Box<dyn Activator> = cuvm_platform::new_activator(Os::Linux);` compiles for both `Os` variants; (b) factory returns the matching backend per `Os` (table test). Since the stub methods are uniform, we assert dispatch by a behavior that differs between backends: `supports(Shell::Bash)` is `true` only for the unix backend, and `supports(Shell::PowerShell)` is `true` only for the windows backend. Add to `lib.rs`:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use cuvm_app::{Activator, Installer};
      use cuvm_core::{Os, Shell};

      #[test]
      fn new_activator_returns_boxed_trait_object_for_both_os() {
          let _linux: Box<dyn Activator> = new_activator(Os::Linux);
          let _win: Box<dyn Activator> = new_activator(Os::Windows);
      }

      #[test]
      fn new_installer_returns_boxed_trait_object_for_both_os() {
          let _linux: Box<dyn Installer> = new_installer(Os::Linux);
          let _win: Box<dyn Installer> = new_installer(Os::Windows);
      }

      #[test]
      fn factory_dispatches_activator_by_os() {
          // Table: (Os, shell-only-the-matching-backend-supports)
          let cases = [
              (Os::Linux, Shell::Bash, Shell::PowerShell),
              (Os::Windows, Shell::PowerShell, Shell::Bash),
          ];
          for (os, supported, foreign) in cases {
              let a = new_activator(os);
              assert!(
                  a.supports(supported),
                  "{os:?} backend must support its own shell"
              );
              assert!(
                  !a.supports(foreign),
                  "{os:?} backend must not claim the other OS's shell"
              );
          }
      }

      #[test]
      fn factory_dispatches_installer_by_os() {
          // Both stubs error identically; assert the error names the right backend type.
          let linux_err = new_installer(Os::Linux)
              .scan()
              .unwrap_err()
              .to_string();
          assert!(linux_err.contains("UnixInstaller"));
          let win_err = new_installer(Os::Windows)
              .scan()
              .unwrap_err()
              .to_string();
          assert!(win_err.contains("WindowsInstaller"));
      }
  }
  ```

- [ ] **Step:** Run it, see it fail (factory fns absent):

  ```bash
  cargo test -p cuvm-platform tests::
  ```

  **Expected:** fail — `error[E0425]: cannot find function \`new_activator\` in this scope` (and `new_installer`). Compilation error.

- [ ] **Step:** Minimal implementation. Add the two factory fns to `crates/cuvm-platform/src/lib.rs` (above the `#[cfg(test)]` block, below the `not_impl` helper):

  ```rust
  use cuvm_app::{Activator, Installer};
  use cuvm_core::Os;

  use crate::unix::{UnixActivator, UnixInstaller};
  use crate::windows::{WindowsActivator, WindowsInstaller};

  /// Runtime factory: select the Activator backend by `Os` value (not `#[cfg]`),
  /// so every backend compiles on every host and Windows golden tests run on Linux CI.
  pub fn new_activator(os: Os) -> Box<dyn Activator> {
      match os {
          Os::Linux => Box::new(UnixActivator::new()),
          Os::Windows => Box::new(WindowsActivator::new()),
      }
  }

  /// Runtime factory: select the Installer backend by `Os` value.
  pub fn new_installer(os: Os) -> Box<dyn Installer> {
      match os {
          Os::Linux => Box::new(UnixInstaller::new()),
          Os::Windows => Box::new(WindowsInstaller::new()),
      }
  }
  ```

- [ ] **Step:** Run the platform suite, see pass:

  ```bash
  cargo test -p cuvm-platform
  ```

  **Expected:** `test result: ok. 8 passed; 0 failed` (2 unix + 2 windows + 4 factory). The trait-object-compliance and table-dispatch tests are green, proving the seam works on both `Os` variants.

- [ ] **Step:** Commit.

  ```bash
  git add crates/cuvm-platform/src/lib.rs
  git commit -m "feat(platform): runtime new_activator/new_installer factory (Os dispatch)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### Task 1.11 — Workspace-wide green gate + Dependency-Rule guard

**Files:**
- (no source changes) — full-workspace verification

- [ ] **Step:** Write the failing guard test (Dependency Rule, spec §3.2). Create a tiny workspace test that asserts `cuvm-app` does NOT depend on any leaf I/O crate, by parsing `cargo metadata`. Create `crates/cuvm-app/tests/dependency_rule.rs`:

  ```rust
  // The Dependency Rule (spec §3.2): cuvm-app depends ONLY on cuvm-core.
  // We assert it at the dependency-graph level via `cargo metadata`.
  use std::process::Command;

  #[test]
  fn cuvm_app_depends_only_on_cuvm_core() {
      let out = Command::new(env!("CARGO"))
          .args(["metadata", "--no-deps", "--format-version", "1"])
          .output()
          .expect("run cargo metadata");
      assert!(out.status.success(), "cargo metadata failed");
      let json: serde_json::Value =
          serde_json::from_slice(&out.stdout).expect("parse metadata json");

      let pkgs = json["packages"].as_array().unwrap();
      let app = pkgs
          .iter()
          .find(|p| p["name"] == "cuvm-app")
          .expect("cuvm-app package present");

      let internal: Vec<String> = app["dependencies"]
          .as_array()
          .unwrap()
          .iter()
          .map(|d| d["name"].as_str().unwrap().to_string())
          .filter(|n| n.starts_with("cuvm-"))
          .collect();

      assert_eq!(
          internal,
          vec!["cuvm-core".to_string()],
          "cuvm-app must depend on cuvm-core ONLY (got {internal:?})"
      );
  }
  ```

  Add `serde_json` as a dev-dependency to `crates/cuvm-app/Cargo.toml`:

  ```toml
  [dev-dependencies]
  serde_json.workspace = true
  ```

- [ ] **Step:** Run it, see it pass (the rule already holds from Task 1.1 — this codifies it as a regression guard). If WU-0 or a later change accidentally added a leaf dep, this would be the failing step:

  ```bash
  cargo test -p cuvm-app --test dependency_rule
  ```

  **Expected:** `test result: ok. 1 passed; 0 failed`. (If it fails, the message names the offending internal dep — fix `Cargo.toml` before proceeding.)

- [ ] **Step:** Run the entire workspace test suite + clippy as the WU-1 exit gate:

  ```bash
  cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
  ```

  **Expected:** all suites green — `cuvm-core` 19, `cuvm-app` 4 unit + 1 integration, `cuvm-platform` 8 — and clippy reports **0 warnings** (the `-D warnings` turns any lint into an error). Total: 32 passing tests across the three crates.

- [ ] **Step:** Commit.

  ```bash
  git add crates/cuvm-app/tests/dependency_rule.rs crates/cuvm-app/Cargo.toml
  git commit -m "test(app): Dependency-Rule guard (cuvm-app -> cuvm-core only) and WU-1 green gate

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

#### WU-1 Done criteria (all green before unblocking parallel tracks)
- `cargo test --workspace` passes (32 tests): core types parse/order/round-trip; app port types + object-safety; platform stubs + factory dispatch table.
- `cuvm-core` has **zero I/O deps** (only `thiserror`/`serde`/`serde_json`/`time`); the `dependency_rule` integration test proves `cuvm-app → cuvm-core` only.
- `cuvm_platform::new_activator(os)` / `new_installer(os)` return `Box<dyn Activator>` / `Box<dyn Installer>` for **both** `Os::Linux` and `Os::Windows`, dispatched at **runtime**; every stub method returns a `NotImplemented` error.
- `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- All CONTRACT core types (`Version`, `Os`/`Arch`/`Platform`, `Shell`, `Source`, `GpuClass`, `Toolkit`, `Cudnn`, `Companion`, `Bundle`, `Alias`, `Pin`, `Driver`, `EnvPlan`, `Manifest`/`BundleRecord`/`VersionMeta`/`DriverRecord`, `CoreError`/`CompatError`) and all CONTRACT trait ports (`Resolver`, `Activator`, `Installer`, `Inventory`, `RegistryClient`, `DriverProbe`, `CompatEngine`) compile and are referenced by later WUs unchanged. This is the seam: WU-2/3/5/6/7/8 (Linux) and WU-9 (Windows) can now branch off `main` in parallel.

---

### WU-2: Resolver + version-spec grammar

**Depends on:** WU-0 (workspace with `cuvm-core`/`cuvm-app` crates compiling) and WU-1 (trait-port module `cuvm-app::ports` exists with the `Resolver` trait stub, `Resolved`, `ResolveVia`; core types `Version`, `Pin`, `Alias`, `Bundle`, `Toolkit` declared). This WU fleshes out `Version::parse`/`Ord` in `cuvm-core`, the typed `CoreErr::NotInstalled`, and a concrete in-memory `MemResolver` in `cuvm-app` that satisfies the `Resolver` port. **Gates: none.**

All commands assume cwd `/home/daniel/cvm`. The gnu target only is installed (musl/windows added in WU-0's CI lane); every test in this WU is pure logic and runs on the host gnu target.

---

#### Task 2.1 — `Version::parse` accepts dotted numeric strings (incl. 4+-part)

**Files:**
- Modify: `crates/cuvm-core/src/version.rs` (created as a stub in WU-1)
- Modify: `crates/cuvm-core/src/error.rs`
- Modify: `crates/cuvm-core/src/lib.rs`
- Test: `crates/cuvm-core/src/version.rs` (inline `#[cfg(test)]`)

1. - [ ] **Step (test):** Add the failing parse test to the bottom of `crates/cuvm-core/src/version.rs`.
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_two_part() {
        let v = Version::parse("12.4").expect("parse 12.4");
        assert_eq!(v.fields, vec![12, 4]);
        assert_eq!(v.raw, "12.4");
        assert_eq!(v.major(), 12);
    }

    #[test]
    fn parse_three_part_driver() {
        let v = Version::parse("570.124.06").expect("parse driver");
        assert_eq!(v.fields, vec![570, 124, 6]);
        assert_eq!(v.raw, "570.124.06");
    }

    #[test]
    fn parse_five_part_cccl() {
        let v = Version::parse("13.3.3.3.1").expect("parse cccl");
        assert_eq!(v.fields, vec![13, 3, 3, 3, 1]);
        assert_eq!(v.major(), 13);
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(Version::parse("").is_err());
    }

    #[test]
    fn parse_rejects_non_numeric() {
        assert!(Version::parse("12.x").is_err());
        assert!(Version::parse("latest").is_err());
        assert!(Version::parse("12..4").is_err());
    }
}
```

2. - [ ] **Step (run, see fail):** `cargo test -p cuvm-core version::tests::parse`
   Expected: compile error / `panicked ... parse 12.4` — the WU-1 stub `parse` returns `unimplemented!()` (or the `fields`/`major` API is not yet implemented). Fail confirmed.

3. - [ ] **Step (impl):** Replace the body of `crates/cuvm-core/src/version.rs` (above the test module) with the real type. First wire the error variant in `crates/cuvm-core/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreErr {
    #[error("invalid version string {0:?}: {1}")]
    BadVersion(String, &'static str),
    #[error("no installed toolkit matches {spec:?}; run `cuvm install {spec}` to install it")]
    NotInstalled { spec: String },
    #[error("alias cycle detected while expanding {0:?}")]
    AliasCycle(String),
    #[error("alias {0:?} is not defined")]
    UnknownAlias(String),
}

pub type Result<T> = std::result::Result<T, CoreErr>;
```
Then in `crates/cuvm-core/src/version.rs`:
```rust
use std::cmp::Ordering;

use crate::error::{CoreErr, Result};

/// A version compared as a numeric tuple, never lexically.
/// `fields` holds the parsed dotted integers (drivers are 3-part, `cccl` 4+-part);
/// `raw` preserves the exact source string for display/round-trip.
#[derive(Debug, Clone)]
pub struct Version {
    pub fields: Vec<u32>,
    pub raw: String,
}

impl Version {
    pub fn parse(s: &str) -> Result<Self> {
        if s.is_empty() {
            return Err(CoreErr::BadVersion(s.to_string(), "empty"));
        }
        let mut fields = Vec::new();
        for part in s.split('.') {
            if part.is_empty() {
                return Err(CoreErr::BadVersion(s.to_string(), "empty component"));
            }
            let n: u32 = part
                .parse()
                .map_err(|_| CoreErr::BadVersion(s.to_string(), "non-numeric component"))?;
            fields.push(n);
        }
        Ok(Version { fields, raw: s.to_string() })
    }

    pub fn major(&self) -> u32 {
        self.fields.first().copied().unwrap_or(0)
    }
}
```
Register the module + re-export in `crates/cuvm-core/src/lib.rs` (add if not present from WU-1):
```rust
pub mod error;
pub mod version;

pub use error::{CoreErr, Result};
pub use version::Version;
```

4. - [ ] **Step (run, see pass):** `cargo test -p cuvm-core version::tests::parse`
   Expected: `test result: ok. 5 passed; 0 failed`.

5. - [ ] **Step (commit):**
```bash
git add crates/cuvm-core/src/version.rs crates/cuvm-core/src/error.rs crates/cuvm-core/src/lib.rs && git commit -m "feat(core): Version::parse for dotted numeric specs incl 4+-part cccl

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 2.2 — `Version` numeric `Ord` (field-by-field, missing tail = 0)

**Files:**
- Modify: `crates/cuvm-core/src/version.rs`
- Test: `crates/cuvm-core/src/version.rs` (inline)

1. - [ ] **Step (test):** Add the ordering tests inside the existing `mod tests` in `crates/cuvm-core/src/version.rs`. These encode the spec's load-bearing facts: `570.124.06 > 570.26`, `12.x < 13.x`, and tail-padding equality.
```rust
    fn v(s: &str) -> Version {
        Version::parse(s).expect("valid version")
    }

    #[test]
    fn ord_numeric_not_lexical() {
        // 570.124.06 > 570.26 (lexical compare would say "124" < "26" is false anyway,
        // but "124" vs "26" lexically compares '1' < '2' => WRONG; numeric is correct).
        assert!(v("570.124.06") > v("570.26"));
        assert!(v("570.26") < v("570.124.06"));
    }

    #[test]
    fn ord_major_dominates() {
        // `12` must NOT outrank any 13.x.
        assert!(v("13.0") > v("12.9"));
        assert!(v("12") < v("13.3.3.3.1"));
    }

    #[test]
    fn ord_missing_tail_is_zero() {
        assert_eq!(v("12.4"), v("12.4.0"));
        assert_eq!(v("12.4.0.0"), v("12.4"));
        assert!(v("12.4.1") > v("12.4"));
    }

    #[test]
    fn ord_eq_and_hash_consistent() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(v("12.4"));
        assert!(set.contains(&v("12.4.0")));
    }

    #[test]
    fn sort_picks_newest_patch_last() {
        let mut xs = vec![v("12.4.1"), v("12.4.0"), v("12.4.10"), v("12.4.2")];
        xs.sort();
        assert_eq!(xs.last().unwrap().raw, "12.4.10");
    }
```

2. - [ ] **Step (run, see fail):** `cargo test -p cuvm-core version::tests::ord`
   Expected: compile error `binary operation `>` cannot be applied to type `Version`` (no `Ord`/`PartialEq` yet). Fail confirmed.

3. - [ ] **Step (impl):** Add the comparison + hashing impls to `crates/cuvm-core/src/version.rs` (after the `impl Version` block). `PartialEq`/`Hash` are hand-written so they agree with the tail-padding `Ord` (12.4 == 12.4.0 must hash equal).
```rust
impl Version {
    fn cmp_fields(&self, other: &Version) -> Ordering {
        let n = self.fields.len().max(other.fields.len());
        for i in 0..n {
            let a = self.fields.get(i).copied().unwrap_or(0);
            let b = other.fields.get(i).copied().unwrap_or(0);
            match a.cmp(&b) {
                Ordering::Equal => continue,
                non_eq => return non_eq,
            }
        }
        Ordering::Equal
    }

    /// Canonical field view with trailing zeros trimmed — basis for Eq/Hash so that
    /// 12.4, 12.4.0 and 12.4.0.0 are the same value.
    fn canonical(&self) -> &[u32] {
        let mut end = self.fields.len();
        while end > 0 && self.fields[end - 1] == 0 {
            end -= 1;
        }
        &self.fields[..end]
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_fields(other)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp_fields(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl std::hash::Hash for Version {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.canonical().hash(state);
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.raw)
    }
}
```

4. - [ ] **Step (run, see pass):** `cargo test -p cuvm-core version::`
   Expected: `test result: ok.` (parse + ord tests all green).

5. - [ ] **Step (commit):**
```bash
git add crates/cuvm-core/src/version.rs && git commit -m "feat(core): numeric field-by-field Ord/Eq/Hash for Version

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 2.3 — property test: `parse ∘ format` identity

**Files:**
- Modify: `crates/cuvm-core/Cargo.toml` (add `proptest` dev-dependency)
- Test: `crates/cuvm-core/tests/version_prop.rs` (new integration test file)

1. - [ ] **Step (dep):** Add to `crates/cuvm-core/Cargo.toml` under `[dev-dependencies]`:
```toml
[dev-dependencies]
proptest = "1"
```

2. - [ ] **Step (test):** Create `crates/cuvm-core/tests/version_prop.rs`. Property: rendering a `Version`'s `raw` and re-parsing yields field-equal value; and a freshly generated dotted string round-trips through parse → Display → parse.
```rust
use cuvm_core::Version;
use proptest::prelude::*;

/// Generate a dotted numeric string of 1..=5 components, each 0..=9999.
fn version_string() -> impl Strategy<Value = String> {
    prop::collection::vec(0u32..10_000, 1..=5)
        .prop_map(|parts| parts.iter().map(|n| n.to_string()).collect::<Vec<_>>().join("."))
}

proptest! {
    #[test]
    fn parse_then_display_then_parse_is_identity(s in version_string()) {
        let a = Version::parse(&s).expect("generated string parses");
        // Display renders `raw`, which equals the source string.
        prop_assert_eq!(a.to_string(), s.clone());
        let b = Version::parse(&a.to_string()).expect("reparse");
        // Numeric equality (tail-zero tolerant) holds across the round trip.
        prop_assert_eq!(a, b);
    }

    #[test]
    fn ord_is_total_and_antisymmetric(s1 in version_string(), s2 in version_string()) {
        let a = Version::parse(&s1).unwrap();
        let b = Version::parse(&s2).unwrap();
        // Exactly one of <, ==, > holds, and it is symmetric under swap.
        let f = a.cmp(&b);
        let r = b.cmp(&a);
        prop_assert_eq!(f, r.reverse());
    }
}
```

3. - [ ] **Step (run, see fail):** `cargo test -p cuvm-core --test version_prop`
   Expected (before dep wiring is picked up / if Display missing): fail. After Task 2.2 Display exists, the realistic failure is only present if the dev-dep is omitted; once `proptest` resolves, run again. (If it passes immediately because 2.2 already supplied Display, record it as the green from step 4 — the property file is new code, so the first compile is the gate.)

4. - [ ] **Step (run, see pass):** `cargo test -p cuvm-core --test version_prop`
   Expected: `test result: ok. 2 passed` (each runs 256 generated cases).

5. - [ ] **Step (commit):**
```bash
git add crates/cuvm-core/Cargo.toml crates/cuvm-core/tests/version_prop.rs && git commit -m "test(core): proptest parse/format identity + Ord totality for Version

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 2.4 — `MemResolver`: exact / minor / major / latest spec grammar

**Files:**
- Modify: `crates/cuvm-app/Cargo.toml` (add `tempfile`, `assert_fs` dev-deps)
- Create: `crates/cuvm-app/src/resolver.rs`
- Modify: `crates/cuvm-app/src/lib.rs` (register module + re-export)
- Modify: `crates/cuvm-app/src/ports.rs` (ensure `Resolver`, `Resolved`, `ResolveVia` shapes per contract)
- Test: `crates/cuvm-app/src/resolver.rs` (inline)

This task introduces a concrete in-memory implementation of the `Resolver` port over a pre-built list of installed `Bundle`s plus alias/pin maps. The trait itself was declared in WU-1; here we confirm its signature and provide `MemResolver`.

1. - [ ] **Step (dep + ports confirmation):** Add to `crates/cuvm-app/Cargo.toml` under `[dev-dependencies]`:
```toml
[dev-dependencies]
tempfile = "3"
assert_fs = "1"
```
   Confirm `crates/cuvm-app/src/ports.rs` contains exactly (carry forward from WU-1; align if drifted):
```rust
use std::path::Path;

use cuvm_core::{Bundle, Pin, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveVia {
    Exact,
    Minor,
    Major,
    Latest,
    Alias,
    PinFile,
    Default,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub bundle: Bundle,
    pub spec: String,
    pub via: ResolveVia,
    pub pin: Option<Pin>,
}

pub trait Resolver {
    fn resolve(&self, spec: &str) -> Result<Resolved>;
    fn resolve_from_dir(&self, cwd: &Path) -> Result<Option<Resolved>>;
    fn expand_alias(&self, name: &str) -> Result<String>;
    fn find_pin_upward(&self, cwd: &Path) -> Result<Option<Pin>>;
}
```
   (Note: the contract lists the port returning `Result`; here it is `cuvm_core::Result` = `Result<_, CoreErr>`, the typed-error path that carries `NotInstalled`.)

2. - [ ] **Step (test):** Create `crates/cuvm-app/src/resolver.rs` with a fixture helper and the grammar tests. The fixture builds `Bundle`s purely in memory — no fs, no I/O.
```rust
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cuvm_core::{Bundle, CoreErr, Pin, Result, Version};
use time::OffsetDateTime;

use crate::ports::{Resolved, ResolveVia, Resolver};

/// A Resolver backed entirely by an in-memory inventory (no fs/network).
/// `installed` is the set of installed bundles (keyed by their handle == toolkit version).
/// `aliases` maps an alias name to another spec (possibly another alias).
pub struct MemResolver {
    installed: Vec<Bundle>,
    aliases: BTreeMap<String, String>,
}

impl MemResolver {
    pub fn new(installed: Vec<Bundle>, aliases: BTreeMap<String, String>) -> Self {
        MemResolver { installed, aliases }
    }

    /// Installed versions, parsed, ascending.
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

    fn newest_with_prefix(&self, prefix: &[u32]) -> Option<Version> {
        self.installed_versions()
            .into_iter()
            .filter(|v| v.fields.len() >= prefix.len() && v.fields[..prefix.len()] == *prefix)
            .max()
    }
}

#[cfg(test)]
mod test_support {
    use super::*;

    pub fn bundle(ver: &str) -> Bundle {
        use cuvm_core::{Arch, Os, Platform, Source, Toolkit};
        let version = Version::parse(ver).unwrap();
        let toolkit = Toolkit {
            version: version.clone(),
            source: Source::Downloaded,
            root: PathBuf::from(format!("/home/u/.cuvm/versions/{ver}")),
            platform: Platform { os: Os::Linux, arch: Arch::X86_64 },
            components: vec!["cuda_nvcc".into(), "cuda_cudart".into()],
            has_lib64: false,
            installed_at: OffsetDateTime::UNIX_EPOCH,
            checksum: None,
        };
        Bundle { toolkit, cudnn: None, extra: vec![] }
    }

    pub fn resolver(versions: &[&str], aliases: &[(&str, &str)]) -> MemResolver {
        let installed = versions.iter().map(|v| bundle(v)).collect();
        let amap = aliases
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        MemResolver::new(installed, amap)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

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
        assert_eq!(err, CoreErr::NotInstalled { spec: "11.8".into() });
        // message offers the install path.
        assert!(err.to_string().contains("cuvm install 11.8"));
    }

    #[test]
    fn exact_spec_not_installed_is_not_installed() {
        let r = resolver(&["12.4.1"], &[]);
        let err = r.resolve("12.4.2").unwrap_err();
        assert_eq!(err, CoreErr::NotInstalled { spec: "12.4.2".into() });
    }
}
```

3. - [ ] **Step (run, see fail):** `cargo test -p cuvm-app resolver::tests`
   Expected: compile error — `resolve` has no body for the grammar (the WU-1 `Resolver` impl for `MemResolver` does not exist yet). Fail confirmed.

4. - [ ] **Step (impl):** Implement the trait for `MemResolver` in `crates/cuvm-app/src/resolver.rs` (above the test modules). `resolve` dispatches on spec shape: `latest`, then alias name, then a parseable version (exact → minor → major by field count), else `NotInstalled`.
```rust
impl Resolver for MemResolver {
    fn resolve(&self, spec: &str) -> Result<Resolved> {
        // 1. literal "latest"
        if spec == "latest" {
            let v = self
                .installed_versions()
                .into_iter()
                .max()
                .ok_or_else(|| CoreErr::NotInstalled { spec: spec.to_string() })?;
            let bundle = self.bundle_for(&v).expect("version came from inventory");
            return Ok(Resolved { bundle, spec: spec.to_string(), via: ResolveVia::Latest, pin: None });
        }

        // 2. alias name (recursive, cycle-guarded) -> re-resolve the target.
        if self.aliases.contains_key(spec) {
            let target = self.expand_alias(spec)?;
            let mut resolved = self.resolve(&target)?;
            resolved.spec = spec.to_string();
            resolved.via = ResolveVia::Alias;
            return Ok(resolved);
        }

        // 3. version spec by field count.
        let parsed = Version::parse(spec)
            .map_err(|_| CoreErr::NotInstalled { spec: spec.to_string() })?;
        let prefix = parsed.fields.as_slice();
        let via = match prefix.len() {
            1 => ResolveVia::Major,
            2 => ResolveVia::Minor,
            _ => ResolveVia::Exact,
        };
        // Exact (>=3 fields): require numeric equality. Minor/major: newest with prefix.
        let chosen = if via == ResolveVia::Exact {
            self.installed_versions().into_iter().find(|v| *v == parsed)
        } else {
            self.newest_with_prefix(prefix)
        };
        let v = chosen.ok_or_else(|| CoreErr::NotInstalled { spec: spec.to_string() })?;
        let bundle = self.bundle_for(&v).expect("version came from inventory");
        Ok(Resolved { bundle, spec: spec.to_string(), via, pin: None })
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
                    cur = next.clone();
                }
                None => return Ok(cur), // terminal: a non-alias spec (version/latest)
            }
        }
    }

    fn find_pin_upward(&self, cwd: &Path) -> Result<Option<Pin>> {
        // Implemented in Task 2.6; provisional stub keeps the trait object complete.
        let _ = cwd;
        Ok(None)
    }
}
```
   Register in `crates/cuvm-app/src/lib.rs`:
```rust
pub mod ports;
pub mod resolver;

pub use ports::{Resolved, ResolveVia, Resolver};
pub use resolver::MemResolver;
```

5. - [ ] **Step (run, see pass):** `cargo test -p cuvm-app resolver::tests`
   Expected: `test result: ok. 6 passed` (exact / minor / major / latest / two NotInstalled cases).

6. - [ ] **Step (commit):**
```bash
git add crates/cuvm-app/Cargo.toml crates/cuvm-app/src/resolver.rs crates/cuvm-app/src/lib.rs crates/cuvm-app/src/ports.rs && git commit -m "feat(app): MemResolver spec grammar exact/minor/major/latest + typed NotInstalled

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 2.5 — alias expansion: recursive resolution + cycle rejection

**Files:**
- Modify: `crates/cuvm-app/src/resolver.rs` (tests + already-implemented `expand_alias`)
- Test: `crates/cuvm-app/src/resolver.rs` (inline)

The `expand_alias` impl already landed in Task 2.4; this task adds the behavior-locking tests (recursion through a chain, terminal resolution, cycle detection, unknown-alias-passes-through-to-version-parse).

1. - [ ] **Step (test):** Add to `mod tests` in `crates/cuvm-app/src/resolver.rs`.
```rust
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
        assert_eq!(r.expand_alias("loop").unwrap_err(), CoreErr::AliasCycle("loop".into()));
    }

    #[test]
    fn unknown_name_falls_through_to_version_parse() {
        // "12" is not an alias -> treated as a major spec.
        let r = resolver(&["12.9.0"], &[]);
        assert_eq!(r.resolve("12").unwrap().via, ResolveVia::Major);
        // a bogus non-version name with no alias -> NotInstalled.
        assert_eq!(
            r.resolve("nope").unwrap_err(),
            CoreErr::NotInstalled { spec: "nope".into() }
        );
    }
```

2. - [ ] **Step (run, see fail):** `cargo test -p cuvm-app resolver::tests::alias`
   Expected: all alias tests should pass IF Task 2.4's `expand_alias` is correct. To confirm the cycle guard is genuinely exercised, first temporarily break it: comment out the `if seen.iter().any(...)` early-return in `expand_alias`, run `cargo test -p cuvm-app resolver::tests::alias_cycle_is_rejected` and observe a hang/stack-overflow (Expected: test does NOT pass). Then restore the guard.

3. - [ ] **Step (impl):** Restore the cycle guard (if removed in step 2). No new production code is required — `expand_alias` from Task 2.4 already implements seen-set cycle detection and terminal pass-through.

4. - [ ] **Step (run, see pass):** `cargo test -p cuvm-app resolver::tests`
   Expected: `test result: ok.` (all grammar + alias tests green — 12 tests total).

5. - [ ] **Step (commit):**
```bash
git add crates/cuvm-app/src/resolver.rs && git commit -m "test(app): lock alias recursion + cycle rejection in MemResolver

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 2.6 — `find_pin_upward`: walk cwd → fs root for `.cuda-version`

**Files:**
- Modify: `crates/cuvm-app/src/resolver.rs` (real `find_pin_upward` + tests)
- Test: `crates/cuvm-app/tests/find_pin.rs` (new fs-fixture integration test)

`find_pin_upward` is the only fs-touching method; it reads `.cuda-version` files, so its tests use `assert_fs`/`tempfile` real temp dirs. The Pin's `spec` is the trimmed file contents; `file` is the absolute path that supplied it. The walk starts at `cwd` and ascends via `Path::parent()` until a `.cuda-version` is found or the root has no parent.

1. - [ ] **Step (test):** Create `crates/cuvm-app/tests/find_pin.rs`.
```rust
use std::collections::BTreeMap;

use assert_fs::prelude::*;
use cuvm_app::{MemResolver, Resolver};

fn empty_resolver() -> MemResolver {
    MemResolver::new(vec![], BTreeMap::new())
}

#[test]
fn finds_pin_in_cwd() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child(".cuda-version").write_str("12.4\n").unwrap();
    let r = empty_resolver();
    let pin = r.find_pin_upward(tmp.path()).unwrap().expect("pin found");
    assert_eq!(pin.spec, "12.4"); // trimmed
    assert_eq!(pin.file, tmp.child(".cuda-version").path());
}

#[test]
fn finds_pin_in_parent() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child(".cuda-version").write_str("13.0.0").unwrap();
    let nested = tmp.child("a/b/c");
    nested.create_dir_all().unwrap();
    let r = empty_resolver();
    let pin = r.find_pin_upward(nested.path()).unwrap().expect("pin found upward");
    assert_eq!(pin.spec, "13.0.0");
    assert_eq!(pin.file, tmp.child(".cuda-version").path());
}

#[test]
fn nearest_pin_wins() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child(".cuda-version").write_str("12.0").unwrap();
    let nested = tmp.child("proj");
    nested.create_dir_all().unwrap();
    nested.child(".cuda-version").write_str("13.3").unwrap();
    let r = empty_resolver();
    let pin = r.find_pin_upward(nested.path()).unwrap().unwrap();
    assert_eq!(pin.spec, "13.3"); // nearer file shadows the ancestor
}

#[test]
fn no_pin_stops_at_fs_root() {
    // A temp dir with no .cuda-version anywhere up to the real fs root.
    let tmp = assert_fs::TempDir::new().unwrap();
    let deep = tmp.child("x/y");
    deep.create_dir_all().unwrap();
    let r = empty_resolver();
    // Must terminate (no infinite loop) and return Ok(None).
    assert!(r.find_pin_upward(deep.path()).unwrap().is_none());
}

#[test]
fn blank_pin_file_is_none_spec_trimmed() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child(".cuda-version").write_str("   \n").unwrap();
    let r = empty_resolver();
    // Whitespace-only file is treated as "no usable pin here" -> keep walking.
    assert!(r.find_pin_upward(tmp.path()).unwrap().is_none());
}
```

2. - [ ] **Step (run, see fail):** `cargo test -p cuvm-app --test find_pin`
   Expected: failures — the provisional `find_pin_upward` stub from Task 2.4 always returns `Ok(None)`, so `finds_pin_in_cwd` panics at `.expect("pin found")`. Fail confirmed.

3. - [ ] **Step (impl):** Replace the provisional `find_pin_upward` stub in `crates/cuvm-app/src/resolver.rs`. The trimmed-empty case skips that file and continues upward; map I/O errors to `CoreErr::BadVersion` is wrong (it is not a version error) — instead, since the port returns `cuvm_core::Result`, treat a read error as "not a usable pin here" and continue (a permission glitch must not abort the walk). Add the needed `use std::fs;`.
```rust
    fn find_pin_upward(&self, cwd: &Path) -> Result<Option<Pin>> {
        let mut dir: Option<&Path> = Some(cwd);
        while let Some(d) = dir {
            let candidate = d.join(".cuda-version");
            if let Ok(contents) = std::fs::read_to_string(&candidate) {
                let spec = contents.trim();
                if !spec.is_empty() {
                    return Ok(Some(Pin { spec: spec.to_string(), file: candidate }));
                }
                // blank file: fall through and keep walking upward
            }
            dir = d.parent();
        }
        Ok(None)
    }
```
   Ensure `cuvm_core::Pin` is in scope (already imported at top of `resolver.rs` from Task 2.4: `use cuvm_core::{..., Pin, ...};`).

4. - [ ] **Step (run, see pass):** `cargo test -p cuvm-app --test find_pin`
   Expected: `test result: ok. 5 passed` (cwd, parent, nearest-wins, root-stop, blank-skip).

5. - [ ] **Step (commit):**
```bash
git add crates/cuvm-app/src/resolver.rs crates/cuvm-app/tests/find_pin.rs && git commit -m "feat(app): find_pin_upward walks cwd to fs root for .cuda-version

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 2.7 — `resolve_from_dir`: pin → resolved bundle, else `default`, else None

**Files:**
- Modify: `crates/cuvm-app/src/resolver.rs` (`resolve_from_dir` already landed in 2.4)
- Test: `crates/cuvm-app/tests/resolve_from_dir.rs` (new integration test)

`resolve_from_dir` was implemented in Task 2.4; this task adds the end-to-end fs+grammar tests that bind `find_pin_upward` (2.6) to `resolve` (2.4–2.5), covering the `PinFile`/`Default`/`None` branches and the "pin spec not installed → NotInstalled" surface.

1. - [ ] **Step (test):** Create `crates/cuvm-app/tests/resolve_from_dir.rs`.
```rust
use std::collections::BTreeMap;
use std::path::PathBuf;

use assert_fs::prelude::*;
use cuvm_app::{MemResolver, ResolveVia, Resolver};
use cuvm_core::{
    Arch, Bundle, CoreErr, Os, Platform, Source, Toolkit, Version,
};
use time::OffsetDateTime;

fn bundle(ver: &str) -> Bundle {
    let version = Version::parse(ver).unwrap();
    Bundle {
        toolkit: Toolkit {
            version: version.clone(),
            source: Source::Downloaded,
            root: PathBuf::from(format!("/v/{ver}")),
            platform: Platform { os: Os::Linux, arch: Arch::X86_64 },
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
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    MemResolver::new(installed, amap)
}

#[test]
fn pin_file_resolves_with_pinfile_via() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child(".cuda-version").write_str("12.4").unwrap();
    let r = resolver(&["12.4.0", "12.4.7"], &[]);
    let got = r.resolve_from_dir(tmp.path()).unwrap().expect("resolved from pin");
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
    let got = r.resolve_from_dir(tmp.path()).unwrap().expect("default used");
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
```

2. - [ ] **Step (run, see fail):** `cargo test -p cuvm-app --test resolve_from_dir`
   Expected: the `pin_file_resolves_with_pinfile_via` and `pin_to_uninstalled_version_is_not_installed` tests fail until Task 2.6's real `find_pin_upward` is in place (it is, after 2.6) — running this test file the first time it compiles is the gate. If 2.6 already merged, the realistic failure here is a `via` mismatch only if `resolve_from_dir` did not override `via` to `PinFile`; confirm the override exists in the Task 2.4 impl.

3. - [ ] **Step (impl):** No new production code — `resolve_from_dir` from Task 2.4 already: (a) calls `find_pin_upward`, (b) on `Some(pin)` resolves `pin.spec`, forces `via = PinFile`, attaches the pin; (c) on `None` falls back to the `default` alias with `via = Default`; (d) otherwise returns `Ok(None)`. If step 2 revealed the `via`/`pin` override missing, add it now to match the Task 2.4 listing.

4. - [ ] **Step (run, see pass):** `cargo test -p cuvm-app --test resolve_from_dir`
   Expected: `test result: ok. 4 passed`.

5. - [ ] **Step (commit):**
```bash
git add crates/cuvm-app/tests/resolve_from_dir.rs crates/cuvm-app/src/resolver.rs && git commit -m "test(app): resolve_from_dir pin/default/none integration over real fs

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 2.8 — full-suite green + clippy gate for WU-2 surface

**Files:**
- (verification only — no new files)

1. - [ ] **Step (run):** `cargo test -p cuvm-core -p cuvm-app`
   Expected: all WU-2 unit + integration tests pass (`version::`, `version_prop`, `resolver::tests`, `find_pin`, `resolve_from_dir`) — `test result: ok` for each binary, `0 failed`.

2. - [ ] **Step (run):** `cargo clippy -p cuvm-core -p cuvm-app --all-targets -- -D warnings`
   Expected: `Finished` with no warnings (clean clippy is the workspace gate established in WU-0).

3. - [ ] **Step (run):** `cargo fmt --check -p cuvm-core -p cuvm-app`
   Expected: no diff (formatting clean).

4. - [ ] **Step (commit, only if fmt/clippy produced fixes):**
```bash
git add -A && git commit -m "chore(app,core): clippy/fmt cleanup for WU-2 resolver surface

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

**WU-2 done when:** `Version` parses/orders dotted numeric specs (incl. `13.3.3.3.1`) numerically with `570.124.06 > 570.26` and `12 < 13.x`; `MemResolver` resolves exact/minor(newest patch)/major(newest in line, never higher major)/latest/alias(recursive, cycle-rejected)/`.cuda-version`; `find_pin_upward` walks cwd→fs root and terminates; missing specs yield typed `CoreErr::NotInstalled { spec }` with an "offer install" message; the parse∘format identity property holds. No gates block this WU; it unblocks **WU-8** (command surface uses `Resolver`) and composes with WU-3 (Inventory will later supply the `installed`/`aliases` that `MemResolver` takes as constructor args, replacing the in-memory fixture).

---

### WU-3: Manifest + Inventory state I/O (cuvm-store)

**Depends on:** WU-0 (workspace + members + shared deps), WU-1 (trait-port module skeleton in `cuvm-app`). **Gates:** none.

This WU lands four things, strictly TDD: (1) the serde state types in `cuvm-core` (`Manifest`, `BundleRecord`, `VersionMeta`, `DriverRecord`) with `schema_version` + forward-compat guard, (2) `~/.cuvm` layout resolution honoring `CUVM_HOME`, (3) atomic write (temp + rename) primitive, (4) the `Inventory` trait (declared in `cuvm-app`) implemented as `FsInventory` in `cuvm-store` (load/save/list/deregister/set_alias) over `manifest.json` and per-version `.cuvm-meta.json`.

Contract notes used verbatim: `Source { Adopted, Downloaded, Supplied }`, `Version{fields,raw}`, `Toolkit`, `Bundle`, `Platform`/`Os`/`Arch` from `cuvm-core` (WU-1/WU-2). `OffsetDateTime` from the `time` crate. Adopted entries keep absolute external `path`; downloaded entries store `versions/<ver>` (resolved against `CUVM_HOME` on `list()`).

---

#### Task 3.1 — Wire the three crates' Cargo manifests for serde state I/O

**Files:**
- Modify: `Cargo.toml` (workspace `[workspace.dependencies]`)
- Modify: `crates/cuvm-core/Cargo.toml`
- Modify: `crates/cuvm-app/Cargo.toml`
- Create: `crates/cuvm-store/Cargo.toml`

1. - [ ] Step: Add the shared dep versions to the workspace root so all crates pin one version. Edit `Cargo.toml` `[workspace.dependencies]` to contain (merge with existing WU-0 entries, do not duplicate keys):
```toml
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
time = { version = "0.3", features = ["serde", "serde-well-known", "macros"] }
thiserror = "1"
anyhow = "1"
tempfile = "3"
assert_fs = "1"
insta = { version = "1", features = ["json"] }
```

2. - [ ] Step: Give `cuvm-core` serde + time + thiserror. Edit `crates/cuvm-core/Cargo.toml`:
```toml
[dependencies]
serde = { workspace = true }
time = { workspace = true }
thiserror = { workspace = true }
```

3. - [ ] Step: Give `cuvm-app` anyhow (its `Result` edge) — it already depends on `cuvm-core` from WU-1. Edit `crates/cuvm-app/Cargo.toml`:
```toml
[dependencies]
cuvm-core = { path = "../cuvm-core" }
anyhow = { workspace = true }
```

4. - [ ] Step: Create the `cuvm-store` crate manifest. Write `crates/cuvm-store/Cargo.toml`:
```toml
[package]
name = "cuvm-store"
version = "0.1.0"
edition = "2021"
rust-version = "1.92"

[dependencies]
cuvm-core = { path = "../cuvm-core" }
cuvm-app = { path = "../cuvm-app" }
serde = { workspace = true }
serde_json = { workspace = true }
time = { workspace = true }
thiserror = { workspace = true }
anyhow = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
assert_fs = { workspace = true }
insta = { workspace = true }
```

5. - [ ] Step: Register `cuvm-store` as a workspace member if WU-0 used an explicit list. Confirm `crates/cuvm-store` appears under `[workspace] members` in the root `Cargo.toml` (add it if a literal list is used; skip if WU-0 used the `crates/*` glob).

6. - [ ] Step: Create a placeholder lib so the crate compiles before any code lands. Write `crates/cuvm-store/src/lib.rs`:
```rust
//! cuvm-store: atomic manifest/meta I/O + content-addressed cudnn store.
```

7. - [ ] Step: Verify the workspace resolves and builds. Run `cargo build -p cuvm-store`. Expected: `Compiling cuvm-store v0.1.0` then `Finished` with no errors.

8. - [ ] Step: Commit.
```bash
git add Cargo.toml crates/cuvm-core/Cargo.toml crates/cuvm-app/Cargo.toml crates/cuvm-store/Cargo.toml crates/cuvm-store/src/lib.rs && git commit -m "build(store): scaffold cuvm-store crate with serde/time deps

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 3.2 — Core serde state types with `schema_version` and stable field naming

**Files:**
- Create: `crates/cuvm-core/src/manifest.rs`
- Modify: `crates/cuvm-core/src/lib.rs` (declare module + re-export)
- Test: `crates/cuvm-core/src/manifest.rs` (inline `#[cfg(test)]`)

These are pure data + serde derives, so they live in `cuvm-core` (zero I/O). `Source` already exists from WU-1; here we add `#[derive(Serialize, Deserialize)]` to it (Task 3.2 step 3). The `installed_at` uses `time::OffsetDateTime` with the RFC3339 well-known serde adapter so golden JSON is deterministic.

1. - [ ] Step: Write the failing round-trip + field-name test. Append to a new file `crates/cuvm-core/src/manifest.rs`:
```rust
//! Serde state types: on-disk `manifest.json` and per-version `.cuvm-meta.json`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::Source;

/// Bump when an incompatible on-disk change ships. Reader rejects anything higher.
pub const SCHEMA_VERSION: u32 = 1;

/// Root document at `$CUVM_HOME/manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    #[serde(default)]
    pub bundles: Vec<BundleRecord>,
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub pins: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_driver: Option<DriverRecord>,
}

impl Default for Manifest {
    fn default() -> Self {
        Manifest {
            schema_version: SCHEMA_VERSION,
            bundles: Vec::new(),
            aliases: BTreeMap::new(),
            pins: BTreeMap::new(),
            last_driver: None,
        }
    }
}

/// One installed/adopted bundle row in the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleRecord {
    pub version: String,
    pub source: Source,
    /// Absolute external path for `Adopted`; `versions/<ver>` (relative to CUVM_HOME)
    /// for `Downloaded`/`Supplied`.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cudnn: Option<String>,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub installed_at: OffsetDateTime,
}

/// Sidecar at `$CUVM_HOME/versions/<ver>/.cuvm-meta.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionMeta {
    pub version: String,
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cudnn: Option<String>,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub has_lib64: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub installed_at: OffsetDateTime,
}

/// Last driver probe cached in the manifest for offline `doctor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverRecord {
    pub version: String,
    pub cuda_ceiling: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn sample_manifest() -> Manifest {
        let mut aliases = BTreeMap::new();
        aliases.insert("default".to_string(), "12.4.1".to_string());
        let mut pins = BTreeMap::new();
        pins.insert("/home/u/proj".to_string(), "12.4".to_string());
        Manifest {
            schema_version: SCHEMA_VERSION,
            bundles: vec![BundleRecord {
                version: "12.4.1".to_string(),
                source: Source::Downloaded,
                path: "versions/12.4.1".to_string(),
                cudnn: Some("9.7.0".to_string()),
                components: vec!["cuda_nvcc".to_string(), "cuda_cudart".to_string()],
                sha256: Some("abc123".to_string()),
                installed_at: datetime!(2026-06-08 10:30:00 UTC),
            }],
            aliases,
            pins,
            last_driver: Some(DriverRecord {
                version: "550.54.14".to_string(),
                cuda_ceiling: "12.4".to_string(),
            }),
        }
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let m = sample_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn manifest_uses_snake_case_field_names_and_rfc3339_time() {
        let json = serde_json::to_value(sample_manifest()).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["bundles"][0]["installed_at"], "2026-06-08T10:30:00Z");
        assert_eq!(json["last_driver"]["cuda_ceiling"], "12.4");
    }

    #[test]
    fn version_meta_round_trips() {
        let vm = VersionMeta {
            version: "12.4.1".to_string(),
            source: Source::Downloaded,
            cudnn: None,
            components: vec!["cuda_nvcc".to_string()],
            sha256: Some("deadbeef".to_string()),
            has_lib64: false,
            installed_at: datetime!(2026-06-08 10:30:00 UTC),
        };
        let json = serde_json::to_string(&vm).unwrap();
        let back: VersionMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(vm, back);
        assert!(json.contains("\"has_lib64\":false"));
    }

    #[test]
    fn empty_object_fields_default_to_empty_collections() {
        let m: Manifest = serde_json::from_str(r#"{"schema_version":1}"#).unwrap();
        assert!(m.bundles.is_empty());
        assert!(m.aliases.is_empty());
        assert!(m.pins.is_empty());
        assert!(m.last_driver.is_none());
    }
}
```

2. - [ ] Step: Run it, see it fail (module not declared / `Source` not serde). Run `cargo test -p cuvm-core manifest::`. Expected: fail — `error[E0583]: file not found for module \`manifest\`` is avoided because the file exists, but you will get `error: cannot find ... manifest` is not declared, plus once declared `the trait bound \`Source: Serialize\` is not satisfied`.

3. - [ ] Step: Declare the module and re-export, and make `Source` serde-capable. Edit `crates/cuvm-core/src/lib.rs` to add the module line and re-exports (place with the other `pub mod`/`pub use` lines from WU-1/WU-2):
```rust
pub mod manifest;

pub use manifest::{
    BundleRecord, DriverRecord, Manifest, VersionMeta, SCHEMA_VERSION,
};
```
Then add serde derives to the existing `Source` enum (wherever WU-1 defined it, e.g. `crates/cuvm-core/src/lib.rs` or `src/types.rs`) and force lowercase variant names so JSON is stable:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Adopted,
    Downloaded,
    Supplied,
}
```

4. - [ ] Step: Run tests, see pass. Run `cargo test -p cuvm-core manifest::`. Expected: `test result: ok. 4 passed; 0 failed`.

5. - [ ] Step: Commit.
```bash
git add crates/cuvm-core/src/manifest.rs crates/cuvm-core/src/lib.rs && git commit -m "feat(core): add Manifest/BundleRecord/VersionMeta serde state types

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 3.3 — Forward-compat guard: typed error on newer/corrupt/old schema (no panic)

**Files:**
- Create: `crates/cuvm-store/src/error.rs`
- Modify: `crates/cuvm-store/src/lib.rs` (declare module + re-export)
- Test: `crates/cuvm-store/src/error.rs` (inline `#[cfg(test)]`)

The reader must reject a `schema_version` higher than `SCHEMA_VERSION` (a newer cuvm wrote it) and corrupt JSON, both as typed errors — never a panic.

1. - [ ] Step: Write the failing error-shape test. Append to a new file `crates/cuvm-store/src/error.rs`:
```rust
//! Typed errors for cuvm-store I/O. No panics on bad on-disk data.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("could not resolve CUVM_HOME: {0}")]
    HomeUnresolved(String),

    #[error("manifest at {path} is not valid JSON: {source}")]
    Corrupt {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error(
        "manifest at {path} has schema_version {found}, but this cuvm understands at \
         most {supported}; upgrade cuvm"
    )]
    SchemaTooNew {
        path: PathBuf,
        found: u32,
        supported: u32,
    },

    #[error("i/o error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("no bundle registered with handle {0}")]
    UnknownHandle(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_too_new_message_names_both_versions() {
        let e = StoreError::SchemaTooNew {
            path: PathBuf::from("/x/manifest.json"),
            found: 99,
            supported: 1,
        };
        let msg = e.to_string();
        assert!(msg.contains("99"));
        assert!(msg.contains("upgrade cuvm"));
    }

    #[test]
    fn unknown_handle_message_names_handle() {
        let e = StoreError::UnknownHandle("13.9.9".to_string());
        assert!(e.to_string().contains("13.9.9"));
    }
}
```

2. - [ ] Step: Run it, see it fail (module not declared). Run `cargo test -p cuvm-store error::`. Expected: fail — `error[E0432]: unresolved ... module \`error\` not declared` (the test module is unreachable until declared in `lib.rs`).

3. - [ ] Step: Declare the module in `crates/cuvm-store/src/lib.rs`:
```rust
//! cuvm-store: atomic manifest/meta I/O + content-addressed cudnn store.

pub mod error;

pub use error::{Result, StoreError};
```

4. - [ ] Step: Run tests, see pass. Run `cargo test -p cuvm-store error::`. Expected: `test result: ok. 2 passed; 0 failed`.

5. - [ ] Step: Commit.
```bash
git add crates/cuvm-store/src/error.rs crates/cuvm-store/src/lib.rs && git commit -m "feat(store): typed StoreError with schema-too-new and corrupt guards

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 3.4 — `CUVM_HOME` layout resolution

**Files:**
- Create: `crates/cuvm-store/src/layout.rs`
- Modify: `crates/cuvm-store/src/lib.rs`
- Test: `crates/cuvm-store/src/layout.rs` (inline `#[cfg(test)]`)

Resolves `$CUVM_HOME` (env override) else `~/.cuvm` (Unix) / `%USERPROFILE%\.cuvm` (Windows), and derives the well-known paths (`manifest.json`, `versions/`, `versions/<ver>/.cuvm-meta.json`). To keep the test hermetic and avoid touching real env globals, `resolve_with` takes an explicit env-getter closure; a thin `resolve()` wrapper reads the real environment.

1. - [ ] Step: Write the failing layout test. Append to a new file `crates/cuvm-store/src/layout.rs`:
```rust
//! `$CUVM_HOME` resolution and well-known on-disk paths.

use std::path::{Path, PathBuf};

use crate::error::{Result, StoreError};

/// Resolved on-disk layout rooted at `$CUVM_HOME`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    root: PathBuf,
}

impl Layout {
    /// Construct from an already-known home root.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Layout { root: root.into() }
    }

    /// Resolve using an injected env getter and an injected home-dir fallback.
    /// `get_env("CUVM_HOME")` wins; else `<home_dir>/.cuvm`.
    pub fn resolve_with<F>(get_env: F, home_dir: Option<PathBuf>) -> Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        if let Some(explicit) = get_env("CUVM_HOME") {
            if !explicit.trim().is_empty() {
                return Ok(Layout::new(PathBuf::from(explicit)));
            }
        }
        let home = home_dir.ok_or_else(|| {
            StoreError::HomeUnresolved(
                "no CUVM_HOME and no home directory available".to_string(),
            )
        })?;
        Ok(Layout::new(home.join(".cuvm")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("manifest.json")
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }

    pub fn version_dir(&self, ver: &str) -> PathBuf {
        self.versions_dir().join(ver)
    }

    pub fn meta_path(&self, ver: &str) -> PathBuf {
        self.version_dir(ver).join(".cuvm-meta.json")
    }

    pub fn cudnn_dir(&self) -> PathBuf {
        self.root.join("cudnn")
    }

    /// Resolve a manifest `path` field: absolute (adopted) returned as-is;
    /// relative (`versions/<ver>`) joined against the home root.
    pub fn resolve_record_path(&self, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else {
            self.root.join(p)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuvm_home_env_override_wins() {
        let l = Layout::resolve_with(
            |k| (k == "CUVM_HOME").then(|| "/custom/cuvmhome".to_string()),
            Some(PathBuf::from("/home/u")),
        )
        .unwrap();
        assert_eq!(l.root(), Path::new("/custom/cuvmhome"));
        assert_eq!(l.manifest_path(), Path::new("/custom/cuvmhome/manifest.json"));
    }

    #[test]
    fn falls_back_to_home_dot_cuvm() {
        let l = Layout::resolve_with(|_| None, Some(PathBuf::from("/home/u"))).unwrap();
        assert_eq!(l.root(), Path::new("/home/u/.cuvm"));
        assert_eq!(
            l.meta_path("12.4.1"),
            Path::new("/home/u/.cuvm/versions/12.4.1/.cuvm-meta.json")
        );
    }

    #[test]
    fn empty_cuvm_home_is_ignored_and_falls_back() {
        let l = Layout::resolve_with(
            |k| (k == "CUVM_HOME").then(|| "   ".to_string()),
            Some(PathBuf::from("/home/u")),
        )
        .unwrap();
        assert_eq!(l.root(), Path::new("/home/u/.cuvm"));
    }

    #[test]
    fn no_home_no_env_is_typed_error_not_panic() {
        let err = Layout::resolve_with(|_| None, None).unwrap_err();
        assert!(matches!(err, StoreError::HomeUnresolved(_)));
    }

    #[test]
    fn adopted_absolute_path_kept_relative_path_joined() {
        let l = Layout::new("/home/u/.cuvm");
        assert_eq!(
            l.resolve_record_path("/usr/local/cuda-12.4"),
            Path::new("/usr/local/cuda-12.4")
        );
        assert_eq!(
            l.resolve_record_path("versions/12.4.1"),
            Path::new("/home/u/.cuvm/versions/12.4.1")
        );
    }
}
```

2. - [ ] Step: Run it, see it fail (module not declared). Run `cargo test -p cuvm-store layout::`. Expected: fail — module `layout` not found until declared in `lib.rs`.

3. - [ ] Step: Declare the module and add the real-env wrapper. Edit `crates/cuvm-store/src/lib.rs`:
```rust
pub mod layout;

pub use layout::Layout;

impl Layout {
    /// Resolve from the real process environment and OS home directory.
    pub fn resolve() -> crate::error::Result<Self> {
        Layout::resolve_with(|k| std::env::var(k).ok(), os_home_dir())
    }
}

fn os_home_dir() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(std::path::PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(std::path::PathBuf::from)
    }
}
```
Note: the `#[cfg(windows)]` branch reads `USERPROFILE` per spec §6 (`%USERPROFILE%\.cuvm`); the non-windows branch reads `HOME`. The pure logic in `resolve_with` is tested on all lanes; only this thin syscall-floor wrapper is `#[cfg]`-gated.

4. - [ ] Step: Run tests, see pass. Run `cargo test -p cuvm-store layout::`. Expected: `test result: ok. 5 passed; 0 failed`.

5. - [ ] Step: Commit.
```bash
git add crates/cuvm-store/src/layout.rs crates/cuvm-store/src/lib.rs && git commit -m "feat(store): CUVM_HOME-honoring layout resolution

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 3.5 — Atomic write primitive (temp + fsync + rename)

**Files:**
- Create: `crates/cuvm-store/src/atomic.rs`
- Modify: `crates/cuvm-store/src/lib.rs`
- Test: `crates/cuvm-store/src/atomic.rs` (inline `#[cfg(test)]`, uses `tempfile`)

The atomic-save contract: write to a sibling temp file, fsync it, then rename over the target. On success no temp remains; if the caller's serialization fails before the rename, the original target is untouched. The temp lives in the **same directory** as the target so `rename` is atomic on one filesystem.

1. - [ ] Step: Write the failing atomic-write test. Append to a new file `crates/cuvm-store/src/atomic.rs`:
```rust
//! Atomic file replacement: write temp in the same dir, fsync, rename over target.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use crate::error::{Result, StoreError};

/// Atomically write `bytes` to `target`. Creates parent dirs. On success the
/// temp file is gone and `target` is the new content; on a write failure the
/// temp is cleaned up and `target` is left untouched.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| StoreError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let tmp = temp_sibling(target);
    let res = (|| -> std::io::Result<()> {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        Ok(())
    })();
    if let Err(source) = res {
        let _ = fs::remove_file(&tmp);
        return Err(StoreError::Io {
            path: tmp,
            source,
        });
    }
    fs::rename(&tmp, target).map_err(|source| {
        let _ = fs::remove_file(&tmp);
        StoreError::Io {
            path: target.to_path_buf(),
            source,
        }
    })?;
    Ok(())
}

fn temp_sibling(target: &Path) -> std::path::PathBuf {
    let pid = std::process::id();
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "cuvm".to_string());
    let tmp_name = format!(".{name}.tmp.{pid}");
    match target.parent() {
        Some(p) => p.join(tmp_name),
        None => std::path::PathBuf::from(tmp_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_content_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("manifest.json");
        write_atomic(&target, b"{\"schema_version\":1}").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"{\"schema_version\":1}");
        // no leftover temp siblings
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty(), "found temp leftovers: {leftovers:?}");
    }

    #[test]
    fn overwrites_existing_target_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("manifest.json");
        write_atomic(&target, b"old").unwrap();
        write_atomic(&target, b"new-and-longer").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new-and-longer");
    }

    #[test]
    fn creates_missing_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("versions/12.4.1/.cuvm-meta.json");
        write_atomic(&target, b"{}").unwrap();
        assert!(target.exists());
    }

    #[test]
    fn original_intact_when_rename_target_is_a_directory() {
        // Simulated failure: target path is an existing directory, so rename
        // over it fails; the pre-existing sibling file must remain untouched.
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("keep.json");
        write_atomic(&good, b"precious").unwrap();
        let blocked = dir.path().join("blocked");
        fs::create_dir(&blocked).unwrap();
        let err = write_atomic(&blocked, b"junk").unwrap_err();
        assert!(matches!(err, StoreError::Io { .. }));
        // unrelated file survived and no temp leaked
        assert_eq!(fs::read(&good).unwrap(), b"precious");
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty(), "temp leaked: {leftovers:?}");
    }
}
```

2. - [ ] Step: Run it, see it fail (module not declared). Run `cargo test -p cuvm-store atomic::`. Expected: fail — module `atomic` not found until declared.

3. - [ ] Step: Declare the module in `crates/cuvm-store/src/lib.rs`:
```rust
pub mod atomic;

pub use atomic::write_atomic;
```

4. - [ ] Step: Run tests, see pass. Run `cargo test -p cuvm-store atomic::`. Expected: `test result: ok. 4 passed; 0 failed`.

5. - [ ] Step: Commit.
```bash
git add crates/cuvm-store/src/atomic.rs crates/cuvm-store/src/lib.rs && git commit -m "feat(store): atomic write_atomic (temp + fsync + rename)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 3.6 — Manifest read/write with schema guard (golden JSON)

**Files:**
- Create: `crates/cuvm-store/src/manifest_io.rs`
- Modify: `crates/cuvm-store/src/lib.rs`
- Test: `crates/cuvm-store/src/manifest_io.rs` (inline `#[cfg(test)]`, uses `tempfile` + `insta`)
- Create (by test run): `crates/cuvm-store/src/snapshots/cuvm_store__manifest_io__tests__golden_manifest_json.snap`

`read_manifest` returns `Manifest::default()` when the file is absent (fresh install), rejects `schema_version > SCHEMA_VERSION` with `SchemaTooNew`, and maps invalid JSON to `Corrupt`. `write_manifest` serializes pretty-printed (deterministic ordering: `BTreeMap` for aliases/pins, `Vec` insertion order for bundles) through `write_atomic`. An `insta` golden locks the exact JSON shape.

1. - [ ] Step: Write the failing manifest-io test (round-trip, absent-file, schema guard, corrupt, golden). Append to a new file `crates/cuvm-store/src/manifest_io.rs`:
```rust
//! Read/write `manifest.json` with schema guard and atomic save.

use std::fs;
use std::path::Path;

use cuvm_core::{Manifest, SCHEMA_VERSION};

use crate::atomic::write_atomic;
use crate::error::{Result, StoreError};

/// Read the manifest. Missing file => fresh `Manifest::default()`.
pub fn read_manifest(path: &Path) -> Result<Manifest> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Manifest::default());
        }
        Err(source) => {
            return Err(StoreError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    // Peek schema_version before full deserialize so a newer doc fails loudly,
    // not as a confusing field error.
    let probe: SchemaProbe =
        serde_json::from_slice(&bytes).map_err(|source| StoreError::Corrupt {
            path: path.to_path_buf(),
            source,
        })?;
    if probe.schema_version > SCHEMA_VERSION {
        return Err(StoreError::SchemaTooNew {
            path: path.to_path_buf(),
            found: probe.schema_version,
            supported: SCHEMA_VERSION,
        });
    }
    serde_json::from_slice(&bytes).map_err(|source| StoreError::Corrupt {
        path: path.to_path_buf(),
        source,
    })
}

/// Serialize pretty + atomically replace the manifest file.
pub fn write_manifest(path: &Path, m: &Manifest) -> Result<()> {
    let json = serde_json::to_vec_pretty(m).expect("Manifest is always serializable");
    write_atomic(path, &json)
}

#[derive(serde::Deserialize)]
struct SchemaProbe {
    #[serde(default)]
    schema_version: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_core::{BundleRecord, Source};
    use std::collections::BTreeMap;
    use time::macros::datetime;

    fn sample() -> Manifest {
        let mut aliases = BTreeMap::new();
        aliases.insert("default".to_string(), "12.4.1".to_string());
        Manifest {
            schema_version: SCHEMA_VERSION,
            bundles: vec![BundleRecord {
                version: "12.4.1".to_string(),
                source: Source::Downloaded,
                path: "versions/12.4.1".to_string(),
                cudnn: None,
                components: vec!["cuda_nvcc".to_string(), "cuda_cudart".to_string()],
                sha256: Some("abc".to_string()),
                installed_at: datetime!(2026-06-08 10:30:00 UTC),
            }],
            aliases,
            pins: BTreeMap::new(),
            last_driver: None,
        }
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        write_manifest(&path, &sample()).unwrap();
        let back = read_manifest(&path).unwrap();
        assert_eq!(sample(), back);
    }

    #[test]
    fn absent_file_yields_default_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        let m = read_manifest(&path).unwrap();
        assert_eq!(m, Manifest::default());
    }

    #[test]
    fn newer_schema_is_rejected_not_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        fs::write(&path, br#"{"schema_version":999,"bundles":[]}"#).unwrap();
        let err = read_manifest(&path).unwrap_err();
        assert!(matches!(
            err,
            StoreError::SchemaTooNew { found: 999, supported: 1, .. }
        ));
    }

    #[test]
    fn corrupt_json_is_typed_error_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        fs::write(&path, b"{ this is not json ]").unwrap();
        let err = read_manifest(&path).unwrap_err();
        assert!(matches!(err, StoreError::Corrupt { .. }));
    }

    #[test]
    fn golden_manifest_json() {
        let json = serde_json::to_string_pretty(&sample()).unwrap();
        insta::assert_snapshot!("golden_manifest_json", json);
    }
}
```

2. - [ ] Step: Run it, see it fail (module not declared + missing snapshot). Run `cargo test -p cuvm-store manifest_io::`. Expected: fail — module `manifest_io` not found until declared; after declaring, the `golden_manifest_json` test fails as a new pending snapshot.

3. - [ ] Step: Declare the module in `crates/cuvm-store/src/lib.rs`:
```rust
pub mod manifest_io;

pub use manifest_io::{read_manifest, write_manifest};
```

4. - [ ] Step: Run tests, accept the golden snapshot. Run `cargo test -p cuvm-store manifest_io::` then `cargo insta accept` (or `INSTA_UPDATE=always cargo test -p cuvm-store manifest_io::`). Inspect the accepted snapshot file `crates/cuvm-store/src/snapshots/cuvm_store__manifest_io__tests__golden_manifest_json.snap` to confirm it shows `"schema_version": 1`, `"source": "downloaded"`, `"path": "versions/12.4.1"`, and `"installed_at": "2026-06-08T10:30:00Z"`. Expected after accept: `test result: ok. 5 passed; 0 failed`.

5. - [ ] Step: Commit (include the snapshot).
```bash
git add crates/cuvm-store/src/manifest_io.rs crates/cuvm-store/src/lib.rs crates/cuvm-store/src/snapshots/ && git commit -m "feat(store): manifest read/write with schema guard + golden JSON

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 3.7 — Per-version `.cuvm-meta.json` read/write

**Files:**
- Create: `crates/cuvm-store/src/meta_io.rs`
- Modify: `crates/cuvm-store/src/lib.rs`
- Test: `crates/cuvm-store/src/meta_io.rs` (inline `#[cfg(test)]`, uses `tempfile`)

The per-version sidecar uses the same atomic write; it carries `has_lib64` (false for downloaded redist trees per spec §2.1). `read_meta` maps a missing file to `StoreError::Io { NotFound }` (a meta sidecar is expected to exist once a version dir exists) and corrupt JSON to `Corrupt`.

1. - [ ] Step: Write the failing meta-io test. Append to a new file `crates/cuvm-store/src/meta_io.rs`:
```rust
//! Read/write the per-version `.cuvm-meta.json` sidecar.

use std::fs;
use std::path::Path;

use cuvm_core::VersionMeta;

use crate::atomic::write_atomic;
use crate::error::{Result, StoreError};

pub fn read_meta(path: &Path) -> Result<VersionMeta> {
    let bytes = fs::read(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| StoreError::Corrupt {
        path: path.to_path_buf(),
        source,
    })
}

pub fn write_meta(path: &Path, meta: &VersionMeta) -> Result<()> {
    let json = serde_json::to_vec_pretty(meta).expect("VersionMeta is always serializable");
    write_atomic(path, &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_core::Source;
    use time::macros::datetime;

    fn sample() -> VersionMeta {
        VersionMeta {
            version: "12.4.1".to_string(),
            source: Source::Downloaded,
            cudnn: Some("9.7.0".to_string()),
            components: vec!["cuda_nvcc".to_string()],
            sha256: Some("abc".to_string()),
            has_lib64: false,
            installed_at: datetime!(2026-06-08 10:30:00 UTC),
        }
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("versions/12.4.1/.cuvm-meta.json");
        write_meta(&path, &sample()).unwrap();
        assert_eq!(read_meta(&path).unwrap(), sample());
    }

    #[test]
    fn missing_meta_is_typed_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope/.cuvm-meta.json");
        let err = read_meta(&path).unwrap_err();
        assert!(matches!(err, StoreError::Io { .. }));
    }

    #[test]
    fn corrupt_meta_is_typed_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".cuvm-meta.json");
        fs::write(&path, b"not json").unwrap();
        let err = read_meta(&path).unwrap_err();
        assert!(matches!(err, StoreError::Corrupt { .. }));
    }
}
```

2. - [ ] Step: Run it, see it fail (module not declared). Run `cargo test -p cuvm-store meta_io::`. Expected: fail — module `meta_io` not found until declared.

3. - [ ] Step: Declare the module in `crates/cuvm-store/src/lib.rs`:
```rust
pub mod meta_io;

pub use meta_io::{read_meta, write_meta};
```

4. - [ ] Step: Run tests, see pass. Run `cargo test -p cuvm-store meta_io::`. Expected: `test result: ok. 3 passed; 0 failed`.

5. - [ ] Step: Commit.
```bash
git add crates/cuvm-store/src/meta_io.rs crates/cuvm-store/src/lib.rs && git commit -m "feat(store): per-version .cuvm-meta.json read/write

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 3.8 — Declare the `Inventory` trait port in `cuvm-app`

**Files:**
- Create: `crates/cuvm-app/src/ports/inventory.rs`
- Modify: `crates/cuvm-app/src/ports/mod.rs` (declare submodule + re-export)
- Modify: `crates/cuvm-app/src/lib.rs` (if `ports` not yet wired by WU-1)
- Test: `crates/cuvm-app/src/ports/inventory.rs` (inline `#[cfg(test)]`, compile-only object-safety check)

Per the SHARED CONTRACT, `cuvm-app` declares the trait ports; `cuvm-store` implements `Inventory`. Methods return `anyhow::Result`. The trait must be object-safe (`Box<dyn Inventory>` is used by the composition root in WU-8). This task only declares the trait if WU-1 hasn't already; if WU-1 already created an `Inventory` port, replace its body with this signature.

1. - [ ] Step: Write the failing object-safety test (it forces the trait to exist and be `dyn`-compatible). Append to a new file `crates/cuvm-app/src/ports/inventory.rs`:
```rust
//! `Inventory` port: manifest-backed bundle registry.

use anyhow::Result;
use cuvm_core::{Bundle, Manifest};

pub trait Inventory {
    /// All registered bundles (downloaded paths resolved, adopted kept in place).
    fn list(&self) -> Result<Vec<Bundle>>;
    /// Remove a bundle row by its handle (== toolkit version). Adopted rows are
    /// de-registered only; files are never deleted here (ADR-005).
    fn deregister(&self, handle: &str) -> Result<()>;
    /// Set/overwrite an alias (e.g. `default` -> `12.4.1`).
    fn set_alias(&self, name: &str, target: &str) -> Result<()>;
    /// Load the raw manifest.
    fn load(&self) -> Result<Manifest>;
    /// Atomically persist the manifest.
    fn save(&self, m: &Manifest) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_is_object_safe() {
        fn _assert(_b: &dyn Inventory) {}
    }
}
```

2. - [ ] Step: Run it, see it fail (module not declared). Run `cargo test -p cuvm-app ports::inventory::`. Expected: fail — `inventory` submodule not declared until added to `ports/mod.rs`.

3. - [ ] Step: Declare the submodule and re-export. Edit `crates/cuvm-app/src/ports/mod.rs` (add alongside WU-1's port modules):
```rust
pub mod inventory;

pub use inventory::Inventory;
```
If WU-1 did not create `ports`, also add to `crates/cuvm-app/src/lib.rs`:
```rust
pub mod ports;
pub use ports::Inventory;
```

4. - [ ] Step: Run tests, see pass. Run `cargo test -p cuvm-app ports::inventory::`. Expected: `test result: ok. 1 passed; 0 failed`.

5. - [ ] Step: Commit.
```bash
git add crates/cuvm-app/src/ports/inventory.rs crates/cuvm-app/src/ports/mod.rs crates/cuvm-app/src/lib.rs && git commit -m "feat(app): declare Inventory trait port

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 3.9 — `FsInventory`: implement `Inventory` over the filesystem

**Files:**
- Create: `crates/cuvm-store/src/inventory.rs`
- Modify: `crates/cuvm-store/src/lib.rs`
- Test: `crates/cuvm-store/src/inventory.rs` (inline `#[cfg(test)]`, uses `tempfile`)

`FsInventory` wraps a `Layout`. `load`/`save` delegate to `manifest_io`. `set_alias`/`deregister` are read-modify-write through the atomic save. `list` reconstructs `Bundle` values from `BundleRecord`s: it resolves `path` via `Layout::resolve_record_path` (adopted absolute kept; downloaded `versions/<ver>` joined to home), reads each version's `.cuvm-meta.json` for `has_lib64` when present (falls back to defaults if a sidecar is missing — adopted installs have none), and builds a minimal `Toolkit`/`Bundle`. The exact `Toolkit`/`Bundle`/`Version`/`Platform` constructors come from WU-1/WU-2 in `cuvm-core`; the snippet below uses `Version::parse`, the public struct fields per the CONTRACT, and `current_platform()` (the WU-1 helper) — adjust field names only if WU-1 chose different constructor ergonomics.

1. - [ ] Step: Write the failing `FsInventory` test. Append to a new file `crates/cuvm-store/src/inventory.rs`:
```rust
//! Filesystem-backed `Inventory` implementation.

use cuvm_app::Inventory;
use cuvm_core::{
    current_platform, Bundle, Manifest, Source, Toolkit, Version,
};

use crate::layout::Layout;
use crate::manifest_io::{read_manifest, write_manifest};
use crate::meta_io::read_meta;

pub struct FsInventory {
    layout: Layout,
}

impl FsInventory {
    pub fn new(layout: Layout) -> Self {
        FsInventory { layout }
    }

    fn record_to_bundle(
        &self,
        rec: &cuvm_core::BundleRecord,
    ) -> anyhow::Result<Bundle> {
        let root = self.layout.resolve_record_path(&rec.path);
        let version = Version::parse(&rec.version)?;
        // has_lib64: prefer the sidecar; adopted (no sidecar) defaults to true
        // because native /usr/local installs ship lib64 (spec §2.1).
        let has_lib64 = if rec.source == Source::Adopted {
            true
        } else {
            let meta_path = root.join(".cuvm-meta.json");
            read_meta(&meta_path).map(|m| m.has_lib64).unwrap_or(false)
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
            toolkit,
            cudnn: None,
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

    fn set_alias(&self, name: &str, target: &str) -> anyhow::Result<()> {
        let mut m = read_manifest(&self.layout.manifest_path())?;
        m.aliases.insert(name.to_string(), target.to_string());
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
    use cuvm_core::{BundleRecord, VersionMeta, SCHEMA_VERSION};
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
            inv.load().unwrap().aliases.get("default").map(String::as_str),
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
        crate::meta_io::write_meta(
            &dir.path().join("versions/12.4.1/.cuvm-meta.json"),
            &meta,
        )
        .unwrap();

        let bundles = inv.list().unwrap();
        assert_eq!(bundles.len(), 1);
        let tk = &bundles[0].toolkit;
        assert_eq!(tk.root, dir.path().join("versions/12.4.1"));
        assert_eq!(tk.source, Source::Downloaded);
        assert!(tk.has_lib64);
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
```

2. - [ ] Step: Run it, see it fail (module not declared). Run `cargo test -p cuvm-store inventory::`. Expected: fail — module `inventory` not found until declared.

3. - [ ] Step: Declare the module in `crates/cuvm-store/src/lib.rs`:
```rust
pub mod inventory;

pub use inventory::FsInventory;
```

4. - [ ] Step: Run tests, see pass. Run `cargo test -p cuvm-store inventory::`. Expected: `test result: ok. 7 passed; 0 failed`. If the build fails on `Toolkit`/`Bundle`/`current_platform` names, reconcile with the actual WU-1/WU-2 constructors (the CONTRACT fields are authoritative; only helper names like `current_platform()` may differ).

5. - [ ] Step: Commit.
```bash
git add crates/cuvm-store/src/inventory.rs crates/cuvm-store/src/lib.rs && git commit -m "feat(store): FsInventory implementing Inventory over manifest+meta

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 3.10 — End-to-end I/O test: atomicity leaves no temp, original intact on failure

**Files:**
- Create: `crates/cuvm-store/tests/inventory_e2e.rs`

A crate-level integration test (in `tests/`) exercises the full save → mutate → save cycle through a real `CUVM_HOME` temp dir and proves the two hard guarantees from the WU spec: a successful save leaves **no** `*.tmp.*` sibling, and a write whose target is blocked leaves the previous manifest content intact.

1. - [ ] Step: Write the failing integration test. Write `crates/cuvm-store/tests/inventory_e2e.rs`:
```rust
//! End-to-end: CUVM_HOME resolution + atomic save guarantees.

use std::collections::BTreeMap;
use std::fs;

use cuvm_app::Inventory;
use cuvm_core::{BundleRecord, Manifest, Source, SCHEMA_VERSION};
use cuvm_store::{FsInventory, Layout};
use time::macros::datetime;

fn rec(ver: &str) -> BundleRecord {
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

#[test]
fn cuvm_home_env_drives_layout_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let layout = Layout::resolve_with(
        |k| (k == "CUVM_HOME").then(|| dir.path().to_string_lossy().into_owned()),
        None,
    )
    .unwrap();
    let inv = FsInventory::new(layout);
    inv.set_alias("default", "12.4.1").unwrap();
    assert!(dir.path().join("manifest.json").exists());
}

#[test]
fn successful_save_leaves_no_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let inv = FsInventory::new(Layout::new(dir.path()));
    let mut m = Manifest {
        schema_version: SCHEMA_VERSION,
        ..Manifest::default()
    };
    m.bundles.push(rec("12.4.1"));
    inv.save(&m).unwrap();
    inv.set_alias("default", "12.4.1").unwrap(); // a second R-M-W save

    let leftovers: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp."))
        .collect();
    assert!(leftovers.is_empty(), "temp leaked: {leftovers:?}");
}

#[test]
fn original_manifest_intact_when_save_target_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let inv = FsInventory::new(Layout::new(dir.path()));

    // first good save
    let mut good = Manifest::default();
    good.aliases = {
        let mut a = BTreeMap::new();
        a.insert("default".to_string(), "12.4.1".to_string());
        a
    };
    inv.save(&good).unwrap();
    let original_bytes = fs::read(dir.path().join("manifest.json")).unwrap();

    // Now make the manifest path un-renameable by turning it into a directory's
    // child situation: replace manifest.json with a directory of the same name
    // after capturing the good bytes, then attempt another save -> must error and
    // leave NO temp. (We restore from captured bytes to prove caller can recover.)
    fs::remove_file(dir.path().join("manifest.json")).unwrap();
    fs::create_dir(dir.path().join("manifest.json")).unwrap();

    let err = inv.save(&good).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("manifest.json"));

    // no temp leaked
    let leftovers: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp."))
        .collect();
    assert!(leftovers.is_empty(), "temp leaked: {leftovers:?}");

    // caller-side recovery proves the captured bytes are the intact original
    fs::remove_dir(dir.path().join("manifest.json")).unwrap();
    fs::write(dir.path().join("manifest.json"), &original_bytes).unwrap();
    assert_eq!(inv.load().unwrap(), good);
}
```

2. - [ ] Step: Run it, see it fail if any re-export is missing. Run `cargo test -p cuvm-store --test inventory_e2e`. Expected: fail only if `Layout`/`FsInventory` are not re-exported from `cuvm-store` (already done in 3.4/3.9) — otherwise it should compile; the assertions are the gate. If it fails to compile, ensure `pub use layout::Layout;` and `pub use inventory::FsInventory;` exist in `lib.rs`.

3. - [ ] Step: Minimal fix (only if needed). If the test could not see the types, add the missing `pub use` lines to `crates/cuvm-store/src/lib.rs`. (No new production logic — the guarantees were implemented in Tasks 3.5/3.6/3.9.)

4. - [ ] Step: Run tests, see pass. Run `cargo test -p cuvm-store --test inventory_e2e`. Expected: `test result: ok. 3 passed; 0 failed`.

5. - [ ] Step: Run the whole crate to confirm nothing regressed. Run `cargo test -p cuvm-store`. Expected: all unit + integration tests `ok` (atomic 4, error 2, layout 5, manifest_io 5, meta_io 3, inventory 7, e2e 3).

6. - [ ] Step: Commit.
```bash
git add crates/cuvm-store/tests/inventory_e2e.rs crates/cuvm-store/src/lib.rs && git commit -m "test(store): e2e atomic-save no-temp + original-intact guarantees

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### WU-3 done criteria (all green, no gates)
- `cargo test -p cuvm-core manifest::` and `cargo test -p cuvm-store` both pass.
- `cargo clippy -p cuvm-store --all-targets -- -D warnings` clean.
- Golden `manifest.json` snapshot committed under `crates/cuvm-store/src/snapshots/`.
- Forward-compat: `schema_version > 1` -> `StoreError::SchemaTooNew` (test `newer_schema_is_rejected_not_loaded`); corrupt JSON -> `StoreError::Corrupt` (no panic).
- Atomicity: success leaves no `*.tmp.*` sibling; blocked save leaves the original manifest recoverable.
- Adopted rows keep absolute external paths; downloaded rows resolve to `versions/<ver>` under `CUVM_HOME`.
- `Inventory` (`load`/`save`/`list`/`deregister`/`set_alias`) implemented by `FsInventory` and object-safe for the WU-8 composition root.

---

### WU-4: Linux adopt (scan + adopt-in-place)

**Goal.** Implement the unix `Installer::scan` + `Installer::adopt` backend: discover toolkits under `/usr/local/cuda-*` plus the `/usr/local/cuda` symlink target, validate each is a real toolkit (`bin/nvcc` + `bin/nvcc.profile` present), and register it with `source: Adopted` **in place** (no copy/move). Wire the result into `Inventory` so `cuvm adopt --scan` records candidates and `cuvm ls` shows them. Per ADR-005, `cuvm uninstall`/`deregister` on an adopted install removes it from the manifest but **never deletes the external directory**.

**Contract grounding (do not re-derive).**
- Adopted `/usr/local/cuda-X.Y` installs use **native `lib64/`** (`has_lib64 = true`) and need **no** `lib64 → lib` symlink fix (spec §2.1; that fix is for *downloaded* redist trees only — WU-13).
- `nvcc.profile` self-locates via `$(_HERE_)` so an adopted in-place prefix is already relocatable as-is; adopt records the path verbatim and changes nothing on disk (spec §2.1, §6).
- Trait surface from the SHARED CONTRACT: `Installer::scan(&self) -> Result<Vec<Candidate>>`, `Installer::adopt(&self, c: &Candidate) -> Result<Bundle>`, `Inventory::list/deregister/load/save`.
- `scan` must use a **configurable scan root** (not the literal `/usr/local`) so integration tests can point it at an `assert_fs` fixture tree with empty files mimicking the layout (spec §13: "containers, pre-staged tiny … toolkits"). The default root in the cli composition is `/usr/local`.

To keep `cuvm-core` I/O-free (spec §3), the filesystem walk lives in `cuvm-platform`; `cuvm-core` only gains the pure `Candidate` value type. The `Installer` port already exists from WU-1 with `scan`/`adopt` as `unimplemented!()` stubs; this WU fills the unix impl. `Inventory` (atomic manifest I/O) and the `Manifest`/`BundleRecord`/`VersionMeta` round-trip already exist from WU-3.

---

#### Task 4.1 — `Candidate` core value type + version-from-dir parsing

**Files:**
- Create: `crates/cuvm-core/src/candidate.rs`
- Modify: `crates/cuvm-core/src/lib.rs` (add `pub mod candidate;` and re-export)
- Test: inline `#[cfg(test)]` in `crates/cuvm-core/src/candidate.rs`

A `Candidate` is the pure description of a discovered-but-not-yet-adopted toolkit dir. It carries no I/O. `Candidate::from_dir_name` extracts the version from a `cuda-X.Y[.Z]` directory name (used by `scan`), returning `None` for names that don't match.

- [ ] **Step: Write the failing test.** Add to `crates/cuvm-core/src/candidate.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::Version;
    use crate::platform::{Arch, Os, Platform};
    use std::path::PathBuf;

    fn linux() -> Platform {
        Platform { os: Os::Linux, arch: Arch::X86_64 }
    }

    #[test]
    fn from_dir_name_parses_minor_version() {
        let c = Candidate::from_dir_name("cuda-12.4", PathBuf::from("/usr/local/cuda-12.4"), linux())
            .expect("cuda-12.4 should parse");
        assert_eq!(c.version, Version::parse("12.4").unwrap());
        assert_eq!(c.root, PathBuf::from("/usr/local/cuda-12.4"));
        assert_eq!(c.handle(), "12.4");
    }

    #[test]
    fn from_dir_name_parses_patch_version() {
        let c = Candidate::from_dir_name("cuda-12.4.1", PathBuf::from("/x/cuda-12.4.1"), linux())
            .expect("cuda-12.4.1 should parse");
        assert_eq!(c.version, Version::parse("12.4.1").unwrap());
    }

    #[test]
    fn from_dir_name_rejects_non_cuda_dirs() {
        assert!(Candidate::from_dir_name("cuda", PathBuf::from("/usr/local/cuda"), linux()).is_none());
        assert!(Candidate::from_dir_name("cudnn-9.2", PathBuf::from("/x"), linux()).is_none());
        assert!(Candidate::from_dir_name("cuda-", PathBuf::from("/x"), linux()).is_none());
        assert!(Candidate::from_dir_name("cuda-banana", PathBuf::from("/x"), linux()).is_none());
        assert!(Candidate::from_dir_name("notcuda-12.4", PathBuf::from("/x"), linux()).is_none());
    }
}
```

- [ ] **Step: Run it, see it fail.** `cargo test -p cuvm-core candidate::`
  Expected: fail — `error[E0433]: failed to resolve: use of undeclared type 'Candidate'` (the type/module does not exist yet).

- [ ] **Step: Minimal implementation.** Put at the top of `crates/cuvm-core/src/candidate.rs`:

```rust
//! Pure description of a toolkit directory discovered on disk but not yet adopted.
//! Zero I/O — the actual filesystem walk + validation lives in `cuvm-platform`.

use std::path::PathBuf;

use crate::platform::Platform;
use crate::version::Version;

/// A discovered, validated-on-the-platform-side toolkit directory awaiting adoption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Parsed toolkit version (from the dir name for `/usr/local/cuda-X.Y`).
    pub version: Version,
    /// Absolute path to the toolkit root, recorded verbatim (adopted in place).
    pub root: PathBuf,
    /// Target platform of the host doing the adoption.
    pub platform: Platform,
}

impl Candidate {
    /// Stable handle used as the manifest key for this candidate (== version raw).
    pub fn handle(&self) -> String {
        self.version.raw.clone()
    }

    /// Parse a `cuda-X.Y[.Z]` directory *name* into a [`Candidate`].
    ///
    /// Returns `None` if `name` does not match the `cuda-<version>` shape or the
    /// version part fails to parse. The bare `cuda` symlink name returns `None`
    /// here on purpose — the symlink is resolved to its target dir before this is
    /// called (see `cuvm-platform`'s scan).
    pub fn from_dir_name(name: &str, root: PathBuf, platform: Platform) -> Option<Candidate> {
        let rest = name.strip_prefix("cuda-")?;
        if rest.is_empty() {
            return None;
        }
        let version = Version::parse(rest).ok()?;
        Some(Candidate { version, root, platform })
    }
}
```

  Then in `crates/cuvm-core/src/lib.rs` add:

```rust
pub mod candidate;
pub use candidate::Candidate;
```

- [ ] **Step: Run tests, see pass.** `cargo test -p cuvm-core candidate::`
  Expected: `test result: ok. 3 passed; 0 failed`.

- [ ] **Step: Commit.**

```bash
git add crates/cuvm-core/src/candidate.rs crates/cuvm-core/src/lib.rs && git commit -m "feat(core): add Candidate value type with cuda-X.Y dir-name parsing

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 4.2 — Make the unix `Installer` scan a configurable root

**Files:**
- Modify: `crates/cuvm-platform/src/unix/mod.rs` (constructor with `scan_root`)
- Modify: `crates/cuvm-platform/src/lib.rs` (`new_installer` factory passes the default root)
- Modify: `crates/cuvm-platform/Cargo.toml` (dev-deps `assert_fs`, `tempfile`; dep `time`)
- Test: `crates/cuvm-platform/tests/adopt_unix.rs`

The WU-1 factory returns a `Box<dyn Installer>`. To test the walk against a fixture tree we need the unix installer to hold a configurable scan root, defaulting to `/usr/local`. We add a `with_scan_root` constructor for tests while keeping `new_installer(Os::Linux)` producing the production default.

- [ ] **Step: Write the failing test.** Create `crates/cuvm-platform/tests/adopt_unix.rs`:

```rust
//! Integration tests for the unix adopt backend. These run on the linux/wsl CI
//! lane. They use empty files mimicking the CUDA layout — no real CUDA toolkit.
#![cfg(unix)]

use assert_fs::prelude::*;
use assert_fs::TempDir;

use cuvm_core::platform::{Arch, Os, Platform};
use cuvm_platform::unix::UnixInstaller;

fn linux() -> Platform {
    Platform { os: Os::Linux, arch: Arch::X86_64 }
}

/// Build a fake `/usr/local`-style tree with two valid toolkits.
/// A "valid" toolkit has bin/nvcc + bin/nvcc.profile (empty files are fine).
fn fixture_two_valid() -> TempDir {
    let root = TempDir::new().unwrap();
    for ver in ["12.4", "11.8"] {
        let base = format!("cuda-{ver}");
        root.child(format!("{base}/bin/nvcc")).touch().unwrap();
        root.child(format!("{base}/bin/nvcc.profile")).touch().unwrap();
        root.child(format!("{base}/lib64/libcudart.so")).touch().unwrap();
    }
    root
}

#[test]
fn scan_root_is_configurable_and_finds_two_valid_toolkits() {
    let root = fixture_two_valid();
    let installer = UnixInstaller::with_scan_root(root.path().to_path_buf(), linux());

    let mut found = installer.scan().expect("scan should succeed");
    found.sort_by(|a, b| a.version.cmp(&b.version));

    let versions: Vec<String> = found.iter().map(|c| c.version.raw.clone()).collect();
    assert_eq!(versions, vec!["11.8".to_string(), "12.4".to_string()]);
    // Roots are recorded verbatim under the scan root (adopt-in-place).
    assert_eq!(found[1].root, root.path().join("cuda-12.4"));
}
```

- [ ] **Step: Run it, see it fail.** `cargo test -p cuvm-platform --test adopt_unix`
  Expected: fail — `error[E0599]: no function or associated item named 'with_scan_root' found` (and `scan` still calls `unimplemented!()` from the WU-1 stub).

- [ ] **Step: Minimal implementation.** First add dev/normal deps to `crates/cuvm-platform/Cargo.toml`:

```toml
[dependencies]
cuvm-core = { path = "../cuvm-core" }
cuvm-app = { path = "../cuvm-app" }
anyhow = { workspace = true }
time = { workspace = true, features = ["std"] }

[dev-dependencies]
assert_fs = { workspace = true }
tempfile = { workspace = true }
```

  Then replace the unix installer struct in `crates/cuvm-platform/src/unix/mod.rs` so it holds a scan root. Keep the existing `Installer` impl block; only the struct + constructors change here (the `scan`/`adopt` bodies land in Task 4.3/4.4):

```rust
use std::path::PathBuf;

use cuvm_core::platform::Platform;

pub mod adopt;

/// Unix (Linux/WSL) implementation of the `Installer` port.
pub struct UnixInstaller {
    /// Directory under which `cuda-X.Y` dirs (+ the `cuda` symlink) are sought.
    /// Production default is `/usr/local`; tests inject a fixture root.
    pub(crate) scan_root: PathBuf,
    /// Host platform recorded on adopted candidates.
    pub(crate) platform: Platform,
}

impl UnixInstaller {
    /// Production constructor: scans `/usr/local`.
    pub fn new(platform: Platform) -> Self {
        Self { scan_root: PathBuf::from("/usr/local"), platform }
    }

    /// Test/override constructor: scans an arbitrary root (e.g. an assert_fs tree).
    pub fn with_scan_root(scan_root: PathBuf, platform: Platform) -> Self {
        Self { scan_root, platform }
    }
}
```

  Ensure `crates/cuvm-platform/src/unix/mod.rs` is reachable and `UnixInstaller` is re-exported — in `crates/cuvm-platform/src/lib.rs` confirm/add:

```rust
#[cfg(unix)]
pub mod unix;
```

  and make the factory build the production installer (the factory body from WU-1 is updated to construct `UnixInstaller::new`). In `crates/cuvm-platform/src/lib.rs`:

```rust
use cuvm_app::ports::Installer;
use cuvm_core::platform::{Arch, Os, Platform};

/// Runtime factory for the OS-specific Installer (composition root calls this).
pub fn new_installer(os: Os) -> Box<dyn Installer> {
    let platform = Platform { os, arch: Arch::X86_64 };
    match os {
        #[cfg(unix)]
        Os::Linux => Box::new(unix::UnixInstaller::new(platform)),
        // Windows installer (WU-9/14) and the non-unix Linux stub compile on every host.
        _ => Box::new(stub::StubInstaller),
    }
}
```

  > Note: `scan` still `unimplemented!()` after this step — the test compiles now but panics. That is expected; the real walk lands in Task 4.3. To keep this commit green, this step only needs the *struct + constructors* to compile, so run the build, not the failing test, before committing.

- [ ] **Step: Confirm it compiles.** `cargo build -p cuvm-platform`
  Expected: `Finished` (no errors). The `adopt_unix` test still fails because `scan` is unimplemented — that is resolved in Task 4.3.

- [ ] **Step: Commit.**

```bash
git add crates/cuvm-platform/Cargo.toml crates/cuvm-platform/src/lib.rs crates/cuvm-platform/src/unix/mod.rs crates/cuvm-platform/tests/adopt_unix.rs && git commit -m "feat(platform): give unix Installer a configurable scan root

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 4.3 — Implement `scan`: walk `cuda-*` + resolve `cuda` symlink, validate `bin/nvcc` + `bin/nvcc.profile`

**Files:**
- Create: `crates/cuvm-platform/src/unix/adopt.rs` (validation + walk helpers)
- Modify: `crates/cuvm-platform/src/unix/mod.rs` (implement `Installer::scan`)
- Test: `crates/cuvm-platform/tests/adopt_unix.rs` (extend)

`scan` enumerates entries directly under `scan_root`, parses each `cuda-X.Y` dir into a `Candidate`, resolves the bare `cuda` symlink to its target dir (so a symlinked default is also offered, deduped by resolved path), and **keeps only validated** candidates (`bin/nvcc` and `bin/nvcc.profile` both present and the root being a directory).

- [ ] **Step: Write the failing test.** Append to `crates/cuvm-platform/tests/adopt_unix.rs`:

```rust
#[test]
fn scan_rejects_dirs_missing_nvcc_or_profile() {
    let root = TempDir::new().unwrap();
    // Has nvcc but NOT nvcc.profile -> invalid.
    root.child("cuda-12.0/bin/nvcc").touch().unwrap();
    // Has nvcc.profile but NOT nvcc -> invalid.
    root.child("cuda-12.1/bin/nvcc.profile").touch().unwrap();
    // Empty dir matching the name pattern -> invalid.
    root.child("cuda-12.2/.keep").touch().unwrap();
    // A fully valid one to prove the scanner still returns the good entry.
    root.child("cuda-12.3/bin/nvcc").touch().unwrap();
    root.child("cuda-12.3/bin/nvcc.profile").touch().unwrap();

    let installer = UnixInstaller::with_scan_root(root.path().to_path_buf(), linux());
    let found = installer.scan().unwrap();
    let versions: Vec<String> = found.iter().map(|c| c.version.raw.clone()).collect();
    assert_eq!(versions, vec!["12.3".to_string()]);
}

#[test]
fn scan_resolves_cuda_symlink_target_and_dedups() {
    use std::os::unix::fs::symlink;
    let root = TempDir::new().unwrap();
    root.child("cuda-12.4/bin/nvcc").touch().unwrap();
    root.child("cuda-12.4/bin/nvcc.profile").touch().unwrap();
    // `cuda` -> `cuda-12.4` (the typical default-install symlink).
    symlink(root.path().join("cuda-12.4"), root.path().join("cuda")).unwrap();

    let installer = UnixInstaller::with_scan_root(root.path().to_path_buf(), linux());
    let found = installer.scan().unwrap();
    // The symlink resolves to the same dir as cuda-12.4, so we get exactly ONE candidate.
    assert_eq!(found.len(), 1, "symlink target must be deduped against cuda-12.4");
    assert_eq!(found[0].version.raw, "12.4");
}

#[test]
fn scan_returns_empty_when_root_missing() {
    let installer =
        UnixInstaller::with_scan_root(std::path::PathBuf::from("/nonexistent/cuvm-scan"), linux());
    assert!(installer.scan().unwrap().is_empty());
}
```

- [ ] **Step: Run it, see it fail.** `cargo test -p cuvm-platform --test adopt_unix`
  Expected: fail — the four scan tests panic with `not implemented` (the `unimplemented!()` body) / `with_scan_root` builds but `scan` is still a stub.

- [ ] **Step: Minimal implementation.** Create `crates/cuvm-platform/src/unix/adopt.rs`:

```rust
//! Filesystem-facing helpers for unix adopt (scan + validate). Kept out of
//! cuvm-core so core stays I/O-free.

use std::fs;
use std::path::{Path, PathBuf};

use cuvm_core::candidate::Candidate;
use cuvm_core::platform::Platform;

/// A directory is a real, adoptable toolkit iff it is a directory containing
/// BOTH `bin/nvcc` and `bin/nvcc.profile`. (nvcc.profile is what makes the tree
/// self-locating via `$(_HERE_)`, so its presence is the relocatability signal.)
pub(crate) fn is_valid_toolkit(root: &Path) -> bool {
    root.is_dir()
        && root.join("bin/nvcc").is_file()
        && root.join("bin/nvcc.profile").is_file()
}

/// Enumerate `cuda-X.Y` candidates directly under `scan_root`, plus the resolved
/// target of a `cuda` symlink, keeping only those that validate. Results are
/// deduped by canonicalized root path so a `cuda -> cuda-12.4` symlink does not
/// double-count. A missing/unreadable scan root yields an empty vec (not an error).
pub(crate) fn scan_root(scan_root: &Path, platform: &Platform) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();

    let mut consider = |name: &str, path: PathBuf| {
        if !is_valid_toolkit(&path) {
            return;
        }
        let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.contains(&key) {
            return;
        }
        if let Some(c) = Candidate::from_dir_name(name, path, platform.clone()) {
            seen.push(key);
            out.push(c);
        }
    };

    let entries = match fs::read_dir(scan_root) {
        Ok(e) => e,
        Err(_) => return out, // missing root => nothing to adopt
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();

        if name == "cuda" {
            // Resolve the default-install symlink to its target dir and offer THAT
            // under its real cuda-X.Y name (canonicalize gives the target path).
            if let Ok(target) = fs::canonicalize(&path) {
                if let Some(target_name) = target.file_name().map(|n| n.to_string_lossy().into_owned()) {
                    consider(&target_name, target);
                }
            }
            continue;
        }
        consider(&name, path);
    }

    out
}
```

  Then implement `scan` in the `Installer` impl block in `crates/cuvm-platform/src/unix/mod.rs` (replace the `unimplemented!()` stub):

```rust
impl Installer for UnixInstaller {
    fn scan(&self) -> anyhow::Result<Vec<Candidate>> {
        Ok(adopt::scan_root(&self.scan_root, &self.platform))
    }

    // adopt(...) implemented in Task 4.4; other methods remain WU-1 stubs.
    // (acquire/verify/extract_atomic/place/smoke_test/ingest_supplied unchanged)
}
```

  Add the needed imports at the top of `crates/cuvm-platform/src/unix/mod.rs`:

```rust
use cuvm_app::ports::Installer;
use cuvm_core::candidate::Candidate;
```

- [ ] **Step: Run tests, see pass.** `cargo test -p cuvm-platform --test adopt_unix`
  Expected: `scan_root_is_configurable_and_finds_two_valid_toolkits`, `scan_rejects_dirs_missing_nvcc_or_profile`, `scan_resolves_cuda_symlink_target_and_dedups`, `scan_returns_empty_when_root_missing` all `ok` (adopt tests not added yet). Result: `4 passed`.

- [ ] **Step: Commit.**

```bash
git add crates/cuvm-platform/src/unix/adopt.rs crates/cuvm-platform/src/unix/mod.rs crates/cuvm-platform/tests/adopt_unix.rs && git commit -m "feat(platform): implement unix Installer::scan over /usr/local/cuda-*

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 4.4 — Implement `adopt`: build an in-place `Bundle` with `source: Adopted`, native `lib64`

**Files:**
- Modify: `crates/cuvm-platform/src/unix/adopt.rs` (add `adopt_candidate`)
- Modify: `crates/cuvm-platform/src/unix/mod.rs` (implement `Installer::adopt`)
- Test: `crates/cuvm-platform/tests/adopt_unix.rs` (extend)

`adopt` builds a `Bundle` whose `Toolkit` points at the candidate root **verbatim** — no copy, no move, nothing written to the external dir. `source: Adopted`, `has_lib64 = true` (native `/usr/local/cuda-X.Y` layout; no symlink fix), `checksum: None` (adopted installs can't be checksum-guaranteed — ADR-005 consequence), `installed_at = OffsetDateTime::now_utc()`, `components` left empty (unknown for an adopted tree; the manifest tolerates this).

- [ ] **Step: Write the failing test.** Append to `crates/cuvm-platform/tests/adopt_unix.rs`:

```rust
use cuvm_core::source::Source;

#[test]
fn adopt_builds_in_place_bundle_without_touching_dir() {
    let root = TempDir::new().unwrap();
    let tk = root.child("cuda-12.4");
    tk.child("bin/nvcc").touch().unwrap();
    tk.child("bin/nvcc.profile").touch().unwrap();
    tk.child("lib64/libcudart.so").touch().unwrap();

    let installer = UnixInstaller::with_scan_root(root.path().to_path_buf(), linux());
    let candidate = installer.scan().unwrap().into_iter().next().unwrap();

    let bundle = installer.adopt(&candidate).expect("adopt should succeed");

    assert_eq!(bundle.toolkit.version.raw, "12.4");
    assert_eq!(bundle.toolkit.source, Source::Adopted);
    // Recorded VERBATIM, in place — same path the scan found.
    assert_eq!(bundle.toolkit.root, root.path().join("cuda-12.4"));
    // Native /usr/local layout uses lib64 -> no symlink fix required.
    assert!(bundle.toolkit.has_lib64, "adopted installs are native lib64");
    assert_eq!(bundle.toolkit.checksum, None);
    assert!(bundle.cudnn.is_none());
    assert!(bundle.extra.is_empty());
    assert_eq!(bundle.handle(), "12.4");

    // ADR-005: adopt must NOT mutate the external tree.
    tk.child("bin/nvcc").assert(predicates::path::is_file());
    tk.child("bin/nvcc.profile").assert(predicates::path::is_file());
    tk.child("lib64/libcudart.so").assert(predicates::path::is_file());
}

#[test]
fn adopt_rejects_a_root_that_is_not_a_valid_toolkit() {
    let root = TempDir::new().unwrap();
    root.child("cuda-9.9/.keep").touch().unwrap(); // no bin/nvcc

    let candidate = cuvm_core::candidate::Candidate {
        version: cuvm_core::version::Version::parse("9.9").unwrap(),
        root: root.path().join("cuda-9.9"),
        platform: linux(),
    };
    let installer = UnixInstaller::with_scan_root(root.path().to_path_buf(), linux());
    assert!(installer.adopt(&candidate).is_err(), "invalid root must not adopt");
}
```

  Add `predicates` to `crates/cuvm-platform/Cargo.toml` dev-deps if not already present:

```toml
predicates = { workspace = true }
```

- [ ] **Step: Run it, see it fail.** `cargo test -p cuvm-platform --test adopt_unix`
  Expected: fail — `adopt_builds_in_place_bundle_without_touching_dir` panics with `not implemented` (adopt is still the WU-1 stub).

- [ ] **Step: Minimal implementation.** Add to `crates/cuvm-platform/src/unix/adopt.rs`:

```rust
use anyhow::{bail, Result};
use time::OffsetDateTime;

use cuvm_core::bundle::Bundle;
use cuvm_core::source::Source;
use cuvm_core::toolkit::Toolkit;

/// Build an in-place [`Bundle`] for an already-validated candidate. Does NOT copy,
/// move, or write anything under the candidate root (ADR-005: adopt in place).
pub(crate) fn adopt_candidate(c: &Candidate) -> Result<Bundle> {
    if !is_valid_toolkit(&c.root) {
        bail!(
            "{} is not a valid CUDA toolkit (missing bin/nvcc or bin/nvcc.profile)",
            c.root.display()
        );
    }
    let toolkit = Toolkit {
        version: c.version.clone(),
        source: Source::Adopted,
        root: c.root.clone(),
        platform: c.platform.clone(),
        components: Vec::new(), // unknown for an adopted tree
        has_lib64: true,        // native /usr/local layout; no lib64->lib fix
        installed_at: OffsetDateTime::now_utc(),
        checksum: None, // adopted installs can't be checksum-guaranteed
    };
    Ok(Bundle { toolkit, cudnn: None, extra: Vec::new() })
}
```

  Then implement `adopt` in `crates/cuvm-platform/src/unix/mod.rs`:

```rust
    fn adopt(&self, c: &Candidate) -> anyhow::Result<Bundle> {
        adopt::adopt_candidate(c)
    }
```

  Add the `Bundle` import to `crates/cuvm-platform/src/unix/mod.rs`:

```rust
use cuvm_core::bundle::Bundle;
```

- [ ] **Step: Run tests, see pass.** `cargo test -p cuvm-platform --test adopt_unix`
  Expected: `6 passed; 0 failed` (4 scan + 2 adopt).

- [ ] **Step: Commit.**

```bash
git add crates/cuvm-platform/Cargo.toml crates/cuvm-platform/src/unix/adopt.rs crates/cuvm-platform/src/unix/mod.rs crates/cuvm-platform/tests/adopt_unix.rs && git commit -m "feat(platform): implement unix Installer::adopt in place (ADR-005)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 4.5 — Wire adopt into `Inventory`: register adopted bundles to the manifest

**Files:**
- Modify: `crates/cuvm-cli/src/commands/adopt.rs` (use-case: scan → adopt → save manifest)
- Modify: `crates/cuvm-cli/src/commands/mod.rs` (register the `adopt` subcommand)
- Test: `crates/cuvm-cli/tests/adopt_e2e.rs`

The composition root wires the unix `Installer` to the WU-3 `Inventory`. The `cuvm adopt --scan` flow: `installer.scan()` → for each candidate `installer.adopt()` → fold each `Bundle` into the `Manifest` as a `BundleRecord { source: Adopted, path: <verbatim root> }` → `inventory.save()`. `cuvm ls` then lists them via `inventory.list()`. The cli accepts a hidden `--scan-root` override (env `CUVM_SCAN_ROOT`) so the e2e test drives a fixture tree; `--home` (env `CUVM_HOME`) points the manifest at a temp dir.

- [ ] **Step: Write the failing test.** Create `crates/cuvm-cli/tests/adopt_e2e.rs`:

```rust
//! Black-box e2e for `cuvm adopt --scan` and `cuvm ls` over a fixture tree.
//! Runs on the linux/wsl CI lane. No real CUDA — empty files mimic the layout.
#![cfg(unix)]

use assert_cmd::Command;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use predicates::prelude::*;

fn fixture(scan: &TempDir, ver: &str) {
    scan.child(format!("cuda-{ver}/bin/nvcc")).touch().unwrap();
    scan.child(format!("cuda-{ver}/bin/nvcc.profile")).touch().unwrap();
    scan.child(format!("cuda-{ver}/lib64/libcudart.so")).touch().unwrap();
}

fn cuvm(home: &TempDir, scan: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("cuvm").unwrap();
    cmd.env("CUVM_HOME", home.path())
        .env("CUVM_SCAN_ROOT", scan.path());
    cmd
}

#[test]
fn adopt_scan_records_both_versions_and_ls_shows_them() {
    let home = TempDir::new().unwrap();
    let scan = TempDir::new().unwrap();
    fixture(&scan, "12.4");
    fixture(&scan, "11.8");

    cuvm(&home, &scan)
        .args(["adopt", "--scan"])
        .assert()
        .success()
        .stdout(predicate::str::contains("12.4").and(predicate::str::contains("11.8")));

    // Manifest now persists both as adopted, in place.
    cuvm(&home, &scan)
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("12.4").and(predicate::str::contains("11.8")));

    let manifest = std::fs::read_to_string(home.path().join("manifest.json")).unwrap();
    assert!(manifest.contains("\"adopted\""), "source must be recorded adopted");
    assert!(manifest.contains(&scan.path().join("cuda-12.4").display().to_string()));
}

#[test]
fn deregister_removes_from_manifest_but_keeps_external_dir() {
    let home = TempDir::new().unwrap();
    let scan = TempDir::new().unwrap();
    fixture(&scan, "12.4");

    cuvm(&home, &scan).args(["adopt", "--scan"]).assert().success();

    // uninstall an adopted install => DE-REGISTER only (ADR-005).
    cuvm(&home, &scan).args(["uninstall", "12.4"]).assert().success();

    // Gone from the manifest...
    cuvm(&home, &scan)
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("12.4").not());

    // ...but the external dir + its files are STILL THERE (never deleted).
    scan.child("cuda-12.4/bin/nvcc").assert(predicate::path::is_file());
    scan.child("cuda-12.4/bin/nvcc.profile").assert(predicate::path::is_file());
    scan.child("cuda-12.4/lib64/libcudart.so").assert(predicate::path::is_file());
}
```

- [ ] **Step: Run it, see it fail.** `cargo test -p cuvm-cli --test adopt_e2e`
  Expected: fail — `adopt --scan` is not wired (clap errors `unrecognized subcommand 'adopt'` or the `--scan`/`CUVM_SCAN_ROOT` plumbing is missing), so the assertions on stdout/manifest do not hold.

- [ ] **Step: Minimal implementation.** Create the use-case in `crates/cuvm-cli/src/commands/adopt.rs`:

```rust
//! `cuvm adopt [--scan]` — discover and register existing toolkits in place.

use std::path::PathBuf;

use anyhow::Result;
use time::OffsetDateTime;

use cuvm_app::ports::{Installer, Inventory};
use cuvm_core::bundle::Bundle;
use cuvm_core::manifest::BundleRecord;
use cuvm_core::source::Source;

/// Run `cuvm adopt --scan`: scan, adopt each candidate in place, persist to the
/// manifest, and print the adopted handles. Idempotent: re-adopting an existing
/// handle overwrites its record rather than duplicating it.
pub fn run_scan(installer: &dyn Installer, inventory: &dyn Inventory) -> Result<()> {
    let mut manifest = inventory.load()?;
    let candidates = installer.scan()?;

    for c in &candidates {
        let bundle = installer.adopt(c)?;
        let record = bundle_to_record(&bundle);
        // De-dup by handle: replace any existing record for this version.
        manifest.bundles.retain(|b| b.version != record.version);
        manifest.bundles.push(record);
        println!("adopted {} ({})", bundle.toolkit.version.raw, bundle.toolkit.root.display());
    }
    if candidates.is_empty() {
        println!("no adoptable CUDA toolkits found");
    }
    inventory.save(&manifest)?;
    Ok(())
}

fn bundle_to_record(b: &Bundle) -> BundleRecord {
    BundleRecord {
        version: b.toolkit.version.raw.clone(),
        source: Source::Adopted,
        path: b.toolkit.root.display().to_string(), // verbatim external path
        cudnn: None,
        components: b.toolkit.components.clone(),
        sha256: b.toolkit.checksum.clone(),
        installed_at: b.toolkit.installed_at,
    }
}

/// Resolve the scan root: `CUVM_SCAN_ROOT` override (tests) else `/usr/local`.
pub fn scan_root_override() -> Option<PathBuf> {
    std::env::var_os("CUVM_SCAN_ROOT").map(PathBuf::from)
}

// Touch `OffsetDateTime` so an unused-import lint never trips if record building
// is refactored; `installed_at` already carries it through bundle_to_record.
#[allow(dead_code)]
fn _assert_time_in_scope() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}
```

  Wire the subcommand + the scan-root override into the clap tree in `crates/cuvm-cli/src/commands/mod.rs`. Add the `Adopt` variant and its handler, building the installer with the override when present:

```rust
pub mod adopt;

use anyhow::Result;
use clap::Subcommand;

use cuvm_core::platform::Os;

#[derive(Subcommand)]
pub enum Command {
    /// Discover and register existing CUDA toolkits in place.
    Adopt {
        /// Scan well-known locations (/usr/local/cuda-*) for installs to adopt.
        #[arg(long)]
        scan: bool,
    },
    /// List installed/adopted bundles.
    Ls,
    /// De-register a bundle (adopted installs are not deleted — ADR-005).
    Uninstall { spec: String },
    // ... other subcommands wired in their own WUs
}

impl Command {
    pub fn run(self, inventory: &dyn cuvm_app::ports::Inventory) -> Result<()> {
        match self {
            Command::Adopt { scan: _ } => {
                let installer = build_unix_installer();
                adopt::run_scan(installer.as_ref(), inventory)
            }
            Command::Ls => list(inventory),
            Command::Uninstall { spec } => {
                inventory.deregister(&spec)?;
                println!("deregistered {spec}");
                Ok(())
            }
        }
    }
}

/// Build the unix installer, honoring `CUVM_SCAN_ROOT` (tests) over `/usr/local`.
fn build_unix_installer() -> Box<dyn cuvm_app::ports::Installer> {
    let platform = cuvm_core::platform::Platform {
        os: Os::Linux,
        arch: cuvm_core::platform::Arch::X86_64,
    };
    match adopt::scan_root_override() {
        #[cfg(unix)]
        Some(root) => Box::new(cuvm_platform::unix::UnixInstaller::with_scan_root(root, platform)),
        _ => cuvm_platform::new_installer(Os::Linux),
    }
}

fn list(inventory: &dyn cuvm_app::ports::Inventory) -> Result<()> {
    for b in inventory.list()? {
        println!("{}\t{:?}\t{}", b.toolkit.version.raw, b.toolkit.source, b.toolkit.root.display());
    }
    Ok(())
}
```

  > The cli `main` (from WU-0/WU-8) constructs the `Inventory` from `CUVM_HOME` and calls `Command::run`. That construction already exists; this WU only adds the `Adopt` arm + installer builder. `Inventory::list` deserializes each `BundleRecord` back into a `Bundle` (WU-3), and `Inventory::deregister` removes the record without touching disk outside the manifest (WU-3 guarantees this for adopted paths).

- [ ] **Step: Run tests, see pass.** `cargo test -p cuvm-cli --test adopt_e2e`
  Expected: `adopt_scan_records_both_versions_and_ls_shows_them` and `deregister_removes_from_manifest_but_keeps_external_dir` both `ok`. Result: `2 passed; 0 failed`.

- [ ] **Step: Commit.**

```bash
git add crates/cuvm-cli/src/commands/adopt.rs crates/cuvm-cli/src/commands/mod.rs crates/cuvm-cli/tests/adopt_e2e.rs && git commit -m "feat(cli): wire adopt --scan + ls into Inventory (deregister keeps external dir)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 4.6 — Full-suite gate + relocatability assertion for adopted trees

**Files:**
- Test: `crates/cuvm-platform/tests/adopt_unix.rs` (one relocatability gate test)

The relocatability gate for the *adopt* path (spec §2.1, WU-4 gating): an adopted `/usr/local/cuda-X.Y` already has a **relative** `nvcc.profile` (`TOP = $(_HERE_)/..`) and a **native `lib64`**, so adoption needs no rewrite and no `lib64 → lib` symlink. We assert the recorded bundle reflects native lib64 and that adoption against a fixture whose `nvcc.profile` contains the relative `$(_HERE_)` marker succeeds unchanged.

- [ ] **Step: Write the failing test.** Append to `crates/cuvm-platform/tests/adopt_unix.rs`:

```rust
#[test]
fn adopt_path_is_relocatable_native_lib64_no_rewrite() {
    use std::fs;
    let root = TempDir::new().unwrap();
    let tk = root.child("cuda-13.0");
    tk.child("bin/nvcc").touch().unwrap();
    // nvcc.profile carries the self-locating relative TOP marker (native install).
    let profile = tk.child("bin/nvcc.profile");
    profile.write_str("TOP = $(_HERE_)/..\nLIBRARIES = -L$(TOP)/lib64\n").unwrap();
    tk.child("lib64/libcudart.so").touch().unwrap(); // native lib64, not lib

    let installer = UnixInstaller::with_scan_root(root.path().to_path_buf(), linux());
    let c = installer.scan().unwrap().into_iter().next().unwrap();
    let before = fs::read_to_string(tk.child("bin/nvcc.profile").path()).unwrap();

    let bundle = installer.adopt(&c).unwrap();

    // No lib64->lib fix needed: native lib64 present, has_lib64 == true.
    assert!(bundle.toolkit.has_lib64);
    assert!(tk.child("lib64/libcudart.so").path().is_file());
    assert!(!tk.child("lib").path().exists(), "adopt must not create a lib symlink");
    // nvcc.profile untouched — relocatability is intrinsic, adopt rewrites nothing.
    let after = fs::read_to_string(tk.child("bin/nvcc.profile").path()).unwrap();
    assert_eq!(before, after);
    assert!(after.contains("$(_HERE_)"));
}
```

- [ ] **Step: Run it, see it fail (or pass-on-first-green).** `cargo test -p cuvm-platform --test adopt_unix relocatable`
  Expected: this asserts behavior already implemented in Task 4.4; if `adopt` were to (wrongly) create a `lib` symlink or rewrite `nvcc.profile`, it would fail with `adopt must not create a lib symlink` or the `assert_eq!(before, after)` mismatch. With the Task 4.4 impl it passes — this test is the executable spec of the gate.

- [ ] **Step: Run the whole WU-4 surface, see all pass.** `cargo test -p cuvm-core -p cuvm-platform -p cuvm-cli`
  Expected: candidate unit tests + 7 `adopt_unix` integration tests + 2 `adopt_e2e` tests all `ok`; `0 failed`.

- [ ] **Step: Lint clean.** `cargo clippy -p cuvm-core -p cuvm-platform -p cuvm-cli --all-targets -- -D warnings`
  Expected: `Finished` with no warnings.

- [ ] **Step: Commit.**

```bash
git add crates/cuvm-platform/tests/adopt_unix.rs && git commit -m "test(platform): assert adopt path is relocatable (native lib64, no rewrite)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

**WU-4 done when:** `cuvm adopt --scan` discovers `/usr/local/cuda-*` (+ the `cuda` symlink target), validates each via `bin/nvcc` + `bin/nvcc.profile`, registers them `source: Adopted` in place, `cuvm ls` shows them, and `cuvm uninstall <ver>` de-registers from the manifest while the external dir + files survive (ADR-005). All scan/adopt logic is testable against an `assert_fs` fixture tree of empty files via the injectable scan root (`CUVM_SCAN_ROOT`). Relocatability of the adopt path is asserted (native `lib64`, no rewrite, no `lib` symlink). Next: WU-5 (Linux Activator + `CUVM_INJECTED` cleanup) consumes the `Bundle`s this WU registers.

---

### WU-5: Linux Activator — env-script emission + CUVM_INJECTED cleanup

**Depends on:** WU-0 (workspace + `cuvm-core`/`cuvm-app`/`cuvm-platform` crates exist, gnu target builds), WU-1 (trait ports `Activator` in `cuvm-app`, runtime factory `cuvm_platform::new_activator(os: Os) -> Box<dyn Activator>` with backend stubs), and the SHARED CONTRACT core types (`Version`, `Os`, `Shell`, `Source`, `Platform`, `Toolkit`, `Bundle`, `EnvPlan`). This WU fills in the unix `Activator` body and the `EnvPlan` builder.

**Spec anchors (do not re-derive):** §2.1 (export `CUDA_HOME` + `CUDA_PATH` + `CUDAToolkit_ROOT`; prepend `bin` to PATH, `lib64` to `LD_LIBRARY_PATH`), §2.5 (`CUVM_INJECTED` records exactly the prepended segments; strip precisely those before prepending; **never strip `/usr/lib/wsl/lib`**), §8 (the **exact** emitted bash script shape, incl. the awk dedup one-liner). Backend dispatch is **runtime** (§3) so both backends compile on every host and these golden tests run on the gnu/Linux lane; `#[cfg(unix)]` is confined to the factory wiring, not the renderer.

**Design contract for this WU:**
- `cuvm-core::env_plan::plan_for(bundle: &Bundle) -> EnvPlan` is a **pure** function (zero I/O) that maps a `Bundle` to the OS-neutral `EnvPlan`. It computes `cuda_home` from `bundle.toolkit.root`, sets the three root vars equal, and lists the two prepend segments (`<root>/bin`, `<root>/lib64`) plus `current`/`injected` from `bundle.handle()`.
- `cuvm-platform::unix::activator::UnixActivator` implements `cuvm_app::Activator`. `emit_env` renders the §8 bash/zsh script from an `EnvPlan`; `emit_deactivate` renders strip-only; `supports(Bash|Zsh) == true`, `supports(PowerShell|Cmd) == false`; `hook` is implemented in WU-6 (here it returns the `Bundle`-independent header only — a stub returning `Ok(String::new())` is **not** acceptable; we emit a `supports`-gated error for non-unix shells and defer the actual hook body to WU-6 with an explicit `unimplemented`-free placeholder that returns the no-op comment line, replaced in WU-6).
- The renderer uses **path separator `:`** and `$HOME`-relative literals verbatim from the plan (the plan already holds absolute-style strings; the Activator never re-derives paths).

---

#### Task 5.1 — `EnvPlan` builder in `cuvm-core` (pure mapping from `Bundle`)

**Files:**
- Create: `crates/cuvm-core/src/env_plan.rs`
- Modify: `crates/cuvm-core/src/lib.rs` (add `pub mod env_plan;` and re-export)
- Test: inline `#[cfg(test)]` module in `crates/cuvm-core/src/env_plan.rs`

1. - [ ] **Write the failing test.** Add to the bottom of `crates/cuvm-core/src/env_plan.rs`:
   ```rust
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
   ```

2. - [ ] **Run it, see it fail.**
   `cargo test -p cuvm-core env_plan::`
   Expected: fails to compile — `cannot find function `plan_for` in this scope` (the `fn` does not exist yet).

3. - [ ] **Minimal implementation.** Put at the **top** of `crates/cuvm-core/src/env_plan.rs`:
   ```rust
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
   ```
   Then register the module in `crates/cuvm-core/src/lib.rs`:
   ```rust
   pub mod env_plan;
   pub use env_plan::plan_for;
   ```
   (The `EnvPlan`, `Bundle`, `Toolkit`, etc. structs and `Bundle::handle()` already exist from the WU-0/WU-1 contract scaffolding; `handle()` returns `self.toolkit.version.raw` i.e. `"12.4.1"`.)

4. - [ ] **Run tests, see pass.**
   `cargo test -p cuvm-core env_plan::`
   Expected: `test result: ok. 2 passed; 0 failed`.

5. - [ ] **Commit.**
   ```bash
   git add crates/cuvm-core/src/env_plan.rs crates/cuvm-core/src/lib.rs && git commit -m "feat(core): pure plan_for mapping Bundle -> EnvPlan

Computes CUDA_HOME/CUDA_PATH/CUDAToolkit_ROOT (all = toolkit root) plus the
bin/lib64 prepend segments that become the CUVM_INJECTED breadcrumb (spec 2.5/8).

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
   ```

---

#### Task 5.2 — `UnixActivator::supports` + crate wiring (Bash/Zsh true, pwsh/cmd false)

**Files:**
- Create: `crates/cuvm-platform/src/unix/activator.rs`
- Create/Modify: `crates/cuvm-platform/src/unix/mod.rs` (add `pub mod activator;`)
- Modify: `crates/cuvm-platform/src/lib.rs` (wire `new_activator` for `Os::Linux` to `UnixActivator`)
- Test: inline `#[cfg(test)]` module in `crates/cuvm-platform/src/unix/activator.rs`

1. - [ ] **Write the failing test.** At the bottom of `crates/cuvm-platform/src/unix/activator.rs`:
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;
       use cuvm_app::Activator;
       use cuvm_core::Shell;

       #[test]
       fn supports_only_posix_shells() {
           let a = UnixActivator::new();
           assert!(a.supports(Shell::Bash));
           assert!(a.supports(Shell::Zsh));
           assert!(!a.supports(Shell::PowerShell));
           assert!(!a.supports(Shell::Cmd));
       }
   }
   ```

2. - [ ] **Run it, see it fail.**
   `cargo test -p cuvm-platform unix::activator::tests::supports_only_posix_shells`
   Expected: fails to compile — `cannot find type `UnixActivator` in this scope`.

3. - [ ] **Minimal implementation.** Put at the top of `crates/cuvm-platform/src/unix/activator.rs`:
   ```rust
   //! Linux/WSL (POSIX) Activator: renders bash/zsh env scripts from an
   //! `EnvPlan`. Compiles on every host (runtime dispatch — spec §3); no
   //! `#[cfg]` here, the syscall floor lives elsewhere.

   use anyhow::{bail, Result};
   use cuvm_app::Activator;
   use cuvm_core::{plan_for, Bundle, Shell};

   /// POSIX-shell Activator. Stateless; cheap to construct per invocation.
   #[derive(Debug, Default, Clone, Copy)]
   pub struct UnixActivator;

   impl UnixActivator {
       pub fn new() -> Self {
           UnixActivator
       }
   }

   impl Activator for UnixActivator {
       fn supports(&self, sh: Shell) -> bool {
           matches!(sh, Shell::Bash | Shell::Zsh)
       }

       fn emit_env(&self, _b: &Bundle, _sh: Shell) -> Result<String> {
           bail!("emit_env not yet implemented")
       }

       fn emit_deactivate(&self, _sh: Shell) -> Result<String> {
           bail!("emit_deactivate not yet implemented")
       }

       fn hook(&self, _sh: Shell) -> Result<String> {
           bail!("hook is implemented in WU-6")
       }
   }
   ```
   In `crates/cuvm-platform/src/unix/mod.rs` add:
   ```rust
   pub mod activator;
   ```
   In `crates/cuvm-platform/src/lib.rs`, replace the WU-1 unix stub branch of the factory so `Os::Linux` returns the real type:
   ```rust
   use cuvm_app::Activator;
   use cuvm_core::Os;

   pub fn new_activator(os: Os) -> Box<dyn Activator> {
       match os {
           Os::Linux => Box::new(unix::activator::UnixActivator::new()),
           Os::Windows => Box::new(windows::activator::WindowsActivator::new()), // WU-9
       }
   }
   ```
   (Keep the `windows::activator::WindowsActivator` stub from WU-1 untouched — it still compiles on Linux because dispatch is runtime.)

4. - [ ] **Run tests, see pass.**
   `cargo test -p cuvm-platform unix::activator::tests::supports_only_posix_shells`
   Expected: `test result: ok. 1 passed; 0 failed`.

5. - [ ] **Commit.**
   ```bash
   git add crates/cuvm-platform/src/unix/activator.rs crates/cuvm-platform/src/unix/mod.rs crates/cuvm-platform/src/lib.rs && git commit -m "feat(platform): UnixActivator skeleton + factory wiring

supports() returns true only for Bash/Zsh; new_activator(Os::Linux) now returns
the real UnixActivator (runtime dispatch, spec 3). Render bodies follow.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
   ```

---

#### Task 5.3 — `emit_env`: render the §8 bash/zsh script (golden via insta)

**Files:**
- Modify: `crates/cuvm-platform/src/unix/activator.rs` (implement `emit_env`)
- Modify: `crates/cuvm-platform/Cargo.toml` (add `insta` to `[dev-dependencies]`)
- Test: `crates/cuvm-platform/tests/unix_activator_golden.rs`
- Test (committed snapshots): `crates/cuvm-platform/tests/snapshots/unix_activator_golden__emit_env_bash.snap`, `…__emit_env_zsh.snap`

1. - [ ] **Add the dev-dependency.** In `crates/cuvm-platform/Cargo.toml` under `[dev-dependencies]`:
   ```toml
   insta = { version = "1", features = ["yaml"] }
   tempfile = "3"
   ```
   (`insta` is already listed as a workspace test dep in spec §3.3; this exposes it to `cuvm-platform`'s test target. `tempfile` is used by later edge tests in this WU.)

2. - [ ] **Write the failing golden test.** Create `crates/cuvm-platform/tests/unix_activator_golden.rs`:
   ```rust
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
           platform: Platform { os: Os::Linux, arch: Arch::X86_64 },
           components: vec!["cuda_nvcc".to_string(), "cuda_cudart".to_string()],
           has_lib64: false,
           installed_at: OffsetDateTime::UNIX_EPOCH,
           checksum: None,
       };
       Bundle { toolkit, cudnn: None, extra: vec![] }
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
   ```

3. - [ ] **Run it, see it fail.**
   `cargo test -p cuvm-platform --test unix_activator_golden`
   Expected: tests run but **fail** — `emit_env` currently `bail!`s, so `.unwrap()` panics with `emit_env not yet implemented`.

4. - [ ] **Implement `emit_env`** verbatim to the §8 shape. Replace the `emit_env` body in `crates/cuvm-platform/src/unix/activator.rs`:
   ```rust
   fn emit_env(&self, b: &Bundle, sh: Shell) -> Result<String> {
       if !self.supports(sh) {
           bail!("UnixActivator does not support {sh:?}");
       }
       let plan = plan_for(b);
       Ok(render_env(&plan))
   }
   ```
   Add these free functions to the same file (above the `impl Activator` block). The strip block is the **exact** awk one-liner from spec §8 (the awk filter `!($0 in d)&&NF` keeps every segment that is NOT in the breadcrumb set — so `/usr/lib/wsl/lib`, never being a breadcrumb member, is preserved; the breadcrumb is rebuilt at the end):
   ```rust
   /// The awk program that drops every PATH/LD segment present in `$CUVM_INJECTED`.
   /// `!($0 in d)&&NF` => keep segments not in the breadcrumb set and non-empty;
   /// `/usr/lib/wsl/lib` is never a breadcrumb member, so WSL driver libs survive.
   const STRIP_AWK: &str =
       r#"awk -v RS=: -v ORS=: -v inj="$CUVM_INJECTED" 'BEGIN{n=split(inj,a,":");for(i=1;i<=n;i++)d[a[i]]=1} !($0 in d)&&NF{print}'"#;

   /// Render the bash/zsh strip block: remove prior CUVM_INJECTED segments from
   /// PATH and LD_LIBRARY_PATH FIRST (spec §2.5/§8). Identical for bash and zsh.
   fn render_strip() -> String {
       format!(
           "if [ -n \"${{CUVM_INJECTED:-}}\" ]; then\n\
            \x20\x20PATH=\"$(printf '%%s' \"$PATH\" | {awk} | sed 's/:$//')\"\n\
            \x20\x20LD_LIBRARY_PATH=\"$(printf '%%s' \"${{LD_LIBRARY_PATH:-}}\" | {awk} | sed 's/:$//')\"\n\
            fi\n",
           awk = STRIP_AWK,
       )
   }

   /// Render the full activation script for a POSIX shell from an EnvPlan.
   fn render_env(plan: &cuvm_core::EnvPlan) -> String {
       let mut out = String::new();
       out.push_str(&render_strip());
       out.push_str(&format!("export CUDA_HOME=\"{}\"\n", plan.cuda_home));
       out.push_str(&format!("export CUDA_PATH=\"{}\"\n", plan.cuda_path));
       out.push_str(&format!("export CUDAToolkit_ROOT=\"{}\"\n", plan.toolkit_root));
       // Prepend bin segments to PATH (in order), each ahead of the existing PATH.
       let path_prepend = plan.prepend_path.join(":");
       out.push_str(&format!("export PATH=\"{path_prepend}:$PATH\"\n"));
       // Prepend lib64 to LD_LIBRARY_PATH, guarding the unset case with :-.
       let lib_prepend = plan.prepend_lib.join(":");
       out.push_str(&format!(
           "export LD_LIBRARY_PATH=\"{lib_prepend}:${{LD_LIBRARY_PATH:-}}\"\n"
       ));
       out.push_str(&format!("export CUVM_CURRENT=\"{}\"\n", plan.current));
       // Breadcrumb: exactly the segments we prepended, colon-joined (spec §2.5).
       out.push_str(&format!("export CUVM_INJECTED=\"{}\"\n", plan.injected.join(":")));
       out
   }
   ```
   Note on the `%%s`/`%s` escaping: the literal `printf '%s'` must reach the emitted script, so inside the Rust `format!` it is written `%%s`. The awk constant uses raw string `r#"..."#` so its embedded quotes need no escaping; it is interpolated as `{awk}` (no `%` inside it).

5. - [ ] **Generate + review + accept the snapshots.**
   `INSTA_UPDATE=no cargo test -p cuvm-platform --test unix_activator_golden`
   Expected: insta reports two **new** pending snapshots and the tests fail (no committed `.snap` yet). Then review the exact emitted bytes:
   `cargo insta show crates/cuvm-platform/tests/snapshots/unix_activator_golden__emit_env_bash.snap.new`
   Confirm the body matches the §8 contract exactly, e.g. the bash snapshot inner content (between the insta header `---` and EOF) is:
   ```text
   if [ -n "${CUVM_INJECTED:-}" ]; then
     PATH="$(printf '%s' "$PATH" | awk -v RS=: -v ORS=: -v inj="$CUVM_INJECTED" 'BEGIN{n=split(inj,a,":");for(i=1;i<=n;i++)d[a[i]]=1} !($0 in d)&&NF{print}' | sed 's/:$//')"
     LD_LIBRARY_PATH="$(printf '%s' "${LD_LIBRARY_PATH:-}" | awk -v RS=: -v ORS=: -v inj="$CUVM_INJECTED" 'BEGIN{n=split(inj,a,":");for(i=1;i<=n;i++)d[a[i]]=1} !($0 in d)&&NF{print}' | sed 's/:$//')"
   fi
   export CUDA_HOME="/home/u/.cuvm/versions/12.4.1"
   export CUDA_PATH="/home/u/.cuvm/versions/12.4.1"
   export CUDAToolkit_ROOT="/home/u/.cuvm/versions/12.4.1"
   export PATH="/home/u/.cuvm/versions/12.4.1/bin:$PATH"
   export LD_LIBRARY_PATH="/home/u/.cuvm/versions/12.4.1/lib64:${LD_LIBRARY_PATH:-}"
   export CUVM_CURRENT="12.4.1"
   export CUVM_INJECTED="/home/u/.cuvm/versions/12.4.1/bin:/home/u/.cuvm/versions/12.4.1/lib64"
   ```
   The zsh snapshot is byte-identical (POSIX shells share the contract). Accept both:
   `cargo insta accept`

6. - [ ] **Run tests, see pass.**
   `cargo test -p cuvm-platform --test unix_activator_golden`
   Expected: `test result: ok. 2 passed; 0 failed` (snapshots now match committed files).

7. - [ ] **Commit** (snapshots are committed fixtures).
   ```bash
   git add crates/cuvm-platform/src/unix/activator.rs crates/cuvm-platform/Cargo.toml crates/cuvm-platform/tests/unix_activator_golden.rs crates/cuvm-platform/tests/snapshots/unix_activator_golden__emit_env_bash.snap crates/cuvm-platform/tests/snapshots/unix_activator_golden__emit_env_zsh.snap && git commit -m "feat(platform): UnixActivator::emit_env renders §8 bash/zsh script

Strip prior CUVM_INJECTED from PATH+LD_LIBRARY_PATH via the §8 awk dedup, then
export CUDA_HOME/CUDA_PATH/CUDAToolkit_ROOT, prepend bin/lib64, set CUVM_CURRENT,
rewrite the CUVM_INJECTED breadcrumb. Golden bash/zsh snapshots committed.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
   ```

---

#### Task 5.4 — `emit_deactivate`: strip-only + clear breadcrumb (golden)

**Files:**
- Modify: `crates/cuvm-platform/src/unix/activator.rs` (implement `emit_deactivate`)
- Test: append to `crates/cuvm-platform/tests/unix_activator_golden.rs`
- Test (committed snapshot): `crates/cuvm-platform/tests/snapshots/unix_activator_golden__emit_deactivate_bash.snap`

1. - [ ] **Write the failing golden test.** Append to `crates/cuvm-platform/tests/unix_activator_golden.rs`:
   ```rust
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
   ```

2. - [ ] **Run it, see it fail.**
   `cargo test -p cuvm-platform --test unix_activator_golden emit_deactivate`
   Expected: `emit_deactivate_bash` panics on `.unwrap()` (`emit_deactivate not yet implemented`); `emit_deactivate_rejects_powershell` already passes (the stub bails on everything) — that is fine, it locks behavior.

3. - [ ] **Implement `emit_deactivate`.** Replace the body in `crates/cuvm-platform/src/unix/activator.rs`:
   ```rust
   fn emit_deactivate(&self, sh: Shell) -> Result<String> {
       if !self.supports(sh) {
           bail!("UnixActivator does not support {sh:?}");
       }
       Ok(render_deactivate())
   }
   ```
   Add the renderer next to `render_env` (strip-only: remove the injected segments, then unset the breadcrumb + current; CUDA_HOME/CUDA_PATH/CUDAToolkit_ROOT are unset since nothing is active):
   ```rust
   /// Render a deactivation script: strip the prior CUVM_INJECTED segments and
   /// clear all cuvm-owned vars. Does NOT prepend anything (spec §5 / §8).
   fn render_deactivate() -> String {
       let mut out = String::new();
       out.push_str(&render_strip());
       out.push_str("unset CUDA_HOME CUDA_PATH CUDAToolkit_ROOT\n");
       out.push_str("unset CUVM_CURRENT CUVM_INJECTED\n");
       out
   }
   ```

4. - [ ] **Generate + review + accept the snapshot.**
   `cargo test -p cuvm-platform --test unix_activator_golden emit_deactivate_bash`
   Expected: one new pending snapshot, test fails. Review:
   `cargo insta show crates/cuvm-platform/tests/snapshots/unix_activator_golden__emit_deactivate_bash.snap.new`
   The inner content must be exactly:
   ```text
   if [ -n "${CUVM_INJECTED:-}" ]; then
     PATH="$(printf '%s' "$PATH" | awk -v RS=: -v ORS=: -v inj="$CUVM_INJECTED" 'BEGIN{n=split(inj,a,":");for(i=1;i<=n;i++)d[a[i]]=1} !($0 in d)&&NF{print}' | sed 's/:$//')"
     LD_LIBRARY_PATH="$(printf '%s' "${LD_LIBRARY_PATH:-}" | awk -v RS=: -v ORS=: -v inj="$CUVM_INJECTED" 'BEGIN{n=split(inj,a,":");for(i=1;i<=n;i++)d[a[i]]=1} !($0 in d)&&NF{print}' | sed 's/:$//')"
   fi
   unset CUDA_HOME CUDA_PATH CUDAToolkit_ROOT
   unset CUVM_CURRENT CUVM_INJECTED
   ```
   Accept: `cargo insta accept`

5. - [ ] **Run tests, see pass.**
   `cargo test -p cuvm-platform --test unix_activator_golden emit_deactivate`
   Expected: `test result: ok. 2 passed; 0 failed`.

6. - [ ] **Commit.**
   ```bash
   git add crates/cuvm-platform/src/unix/activator.rs crates/cuvm-platform/tests/unix_activator_golden.rs crates/cuvm-platform/tests/snapshots/unix_activator_golden__emit_deactivate_bash.snap && git commit -m "feat(platform): UnixActivator::emit_deactivate strips + clears breadcrumb

Reuses the §8 strip block, then unsets CUDA_HOME/CUDA_PATH/CUDAToolkit_ROOT and
CUVM_CURRENT/CUVM_INJECTED. Golden snapshot committed; non-POSIX shells rejected.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
   ```

---

#### Task 5.5 — Behavioral proof: run the emitted bash and assert no PATH duplicate stacking on repeated `use`

**Files:**
- Modify: `crates/cuvm-platform/tests/unix_activator_golden.rs` (add a black-box `bash --norc` execution test)

These tests `eval` the actual emitted script in a real `bash` to prove the awk dedup behaves (the snapshot proves the bytes; this proves the semantics). They are gated to run only when `bash` is on PATH (always true on the gnu/Linux lane).

1. - [ ] **Write the failing test.** Append to `crates/cuvm-platform/tests/unix_activator_golden.rs`:
   ```rust
   use std::process::Command;

   /// Run `script` under `bash --norc --noprofile` with the given starting PATH
   /// and LD_LIBRARY_PATH, then echo the resulting PATH / LD_LIBRARY_PATH /
   /// CUVM_INJECTED on separate lines. Returns (path, ld, injected).
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
       assert!(out.status.success(), "bash stderr: {}", String::from_utf8_lossy(&out.stderr));
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
       assert!(out.status.success(), "bash stderr: {}", String::from_utf8_lossy(&out.stderr));
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
   ```

2. - [ ] **Run it, see it fail (then verify it passes against the real impl).**
   `cargo test -p cuvm-platform --test unix_activator_golden repeated_use_does_not_stack_path_duplicates`
   Expected on first run *before* `emit_env` existed it would `bail`; since `emit_env` is now implemented (Task 5.3), this test should **pass immediately** — which is the intended TDD outcome here: the behavior is already correct by construction and this test locks it. If it fails with a stacked duplicate, the awk strip is wrong; fix `render_strip` before proceeding. (This is the spec §13 "repeated-`use` no PATH duplication" golden.)
   Expected: `test result: ok. 1 passed; 0 failed`.

3. - [ ] **Commit.**
   ```bash
   git add crates/cuvm-platform/tests/unix_activator_golden.rs && git commit -m "test(platform): prove repeated use does not stack PATH/LD duplicates

Evals the emitted bash twice under bash --norc; asserts the bin/lib64 segments
appear exactly once after re-activation (spec §13 awk-dedup behaviour).

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
   ```

---

#### Task 5.6 — WSL driver-path preserved + empty-PATH edge + breadcrumb-drift fallback

**Files:**
- Modify: `crates/cuvm-platform/tests/unix_activator_golden.rs` (three behavioral tests)

1. - [ ] **Write the failing tests.** Append to `crates/cuvm-platform/tests/unix_activator_golden.rs`:
   ```rust
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
       assert!(out.status.success(), "bash stderr: {}", String::from_utf8_lossy(&out.stderr));
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
       let out = Command::new("bash")
           .args(["--norc", "--noprofile", "-c", &program])
           .env("PATH", "")
           .env_remove("LD_LIBRARY_PATH")
           .output()
           .unwrap();
       assert!(out.status.success(), "bash stderr: {}", String::from_utf8_lossy(&out.stderr));
       let s = String::from_utf8(out.stdout).unwrap();
       let mut lines = s.lines();
       let path = lines.next().unwrap();
       let ld = lines.next().unwrap();
       // PATH = "<bin>:" + (empty stripped to "") => no trailing ":" leaks, and
       // no leading "::" appears. We assert there is no empty segment.
       assert!(!path.contains("::"), "double-separator in PATH: {path}");
       assert!(!path.trim_end_matches(']').ends_with(':'), "trailing sep: {path}");
       // LD_LIBRARY_PATH started unset => result is "<lib64>:" with the :- guard
       // producing an empty tail; assert lib64 present and no double-sep.
       assert!(ld.contains("/lib64"), "lib64 missing: {ld}");
       assert!(!ld.contains("::"), "double-separator in LD: {ld}");
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

       let program = format!(
           "export CUVM_INJECTED='{stale}'\n{script}\nprintf '%s\\n' \"$PATH\"\n"
       );
       let out = Command::new("bash")
           .args(["--norc", "--noprofile", "-c", &program])
           .env("PATH", "/usr/bin:/bin:/opt/keep/bin")
           .env_remove("LD_LIBRARY_PATH")
           .output()
           .unwrap();
       assert!(out.status.success(), "bash stderr: {}", String::from_utf8_lossy(&out.stderr));
       let path = String::from_utf8(out.stdout).unwrap();
       let path = path.lines().next().unwrap();
       // Real entries untouched (nothing matched the stale breadcrumb).
       assert!(path.contains("/opt/keep/bin"), "real entry dropped: {path}");
       assert!(path.contains("/usr/bin"), "real entry dropped: {path}");
       // The new bin is prepended exactly once.
       assert_eq!(path.matches(bin).count(), 1, "new bin not prepended once: {path}");
       // The stale 9.9.9 segments were never in PATH, so they are still absent.
       assert!(!path.contains("9.9.9"), "phantom stale segment appeared: {path}");
   }
   ```

2. - [ ] **Run them, see them pass against the real impl.**
   `cargo test -p cuvm-platform --test unix_activator_golden wsl_driver_path_is_never_stripped empty_path_edge_does_not_emit_trailing_separator breadcrumb_drift_stale_injected_strips_nothing_unexpected`
   Expected: `test result: ok. 3 passed; 0 failed`. These lock the spec §2.5 invariants (never strip WSL libs; clean separators on empty PATH; stale-breadcrumb strip-nothing fallback). If `empty_path_edge` shows a `::` or trailing `:`, the `sed 's/:$//'` + awk `NF` guard in `render_strip` is wrong — fix before committing. (The awk `NF` predicate drops empty segments produced by the empty `$PATH`, and `sed 's/:$//'` removes the trailing separator left by `ORS=:`.)

3. - [ ] **Commit.**
   ```bash
   git add crates/cuvm-platform/tests/unix_activator_golden.rs && git commit -m "test(platform): WSL-libs-preserved, empty-PATH edge, breadcrumb-drift fallback

Eval the emitted bash to prove: /usr/lib/wsl/lib is never stripped (spec §2.5),
empty PATH yields no double/trailing separators, and a stale CUVM_INJECTED strips
nothing real (strip-nothing fallback). Closes the WU-5 shim-protocol gate.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
   ```

---

#### Task 5.7 — Gate: full crate green + clippy + fmt (shim-protocol checkpoint)

**Files:** none (verification only)

1. - [ ] **Run the whole crate's tests.**
   `cargo test -p cuvm-platform`
   Expected: all unit tests (`unix::activator::tests::*`) and the `unix_activator_golden` integration tests pass — `test result: ok` for every binary; 0 failed total.

2. - [ ] **Run the core crate's tests** (the `plan_for` mapping is part of this WU's seam).
   `cargo test -p cuvm-core env_plan::`
   Expected: `test result: ok. 2 passed; 0 failed`.

3. - [ ] **Lint + format gate** (deny warnings; this WU's code uses only stable idioms).
   `cargo clippy -p cuvm-platform -p cuvm-core --all-targets -- -D warnings && cargo fmt --check`
   Expected: clippy exits 0 with no warnings; `cargo fmt --check` prints nothing and exits 0. If `fmt --check` fails, run `cargo fmt` and amend.

4. - [ ] **Confirm runtime dispatch parity** (both backends still compile on the Linux host — the spec §3 invariant that lets Windows golden tests run on Linux later in WU-9).
   `cargo build -p cuvm-platform`
   Expected: builds clean (the `windows::activator::WindowsActivator` stub compiles on Linux because dispatch is runtime; no `#[cfg]` was added in this WU).

5. - [ ] **Commit the gate marker** (empty tree-allowed gate commit only if fmt/clippy required fixups; otherwise skip — nothing to commit).
   ```bash
   git add -A && git commit -m "chore(platform): WU-5 gate — clippy/fmt clean, both backends compile

Shim-protocol gate for the Linux Activator: emit_env/emit_deactivate golden +
behavioral suite green; runtime dispatch keeps the Windows stub compiling on Linux.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
   ```
   (If `git status` is clean after steps 1–4, skip this commit — the work is already committed in 5.1–5.6.)

---

**WU-5 done when:** `cargo test -p cuvm-platform -p cuvm-core` is green; the three committed `.snap` golden files (`emit_env_bash`, `emit_env_zsh`, `emit_deactivate_bash`) match the §8 contract byte-for-byte; the five behavioral `bash --norc` tests prove no PATH stacking, WSL-libs preserved, clean empty-PATH separators, and stale-breadcrumb strip-nothing fallback; `new_activator(Os::Linux)` returns the real `UnixActivator` via runtime dispatch. This satisfies the WU-5 shim-protocol gate and unblocks WU-6 (`hook` + shim install) and WU-8 (`use`/`current`/`default` wiring).

---

### WU-6: Unix shims + hook + env/hook plumbing commands

**Depends on:** WU-0 (workspace + `cuvm-cli` clap skeleton + composition root), WU-1 (`Activator` trait port + `cuvm_platform::new_activator(os)` runtime factory + backend stubs), WU-2 (`Resolver` + `cuvm env` spec resolution), WU-5 (Unix `Activator` impl: `emit_env`, `emit_deactivate`, `hook`, `supports`). WU-6 wires two hidden CLI subcommands (`env`, `hook`) to the already-implemented `Activator`, then authors and embeds the Unix shell shims (`cuvm.sh` / `cuvm.zsh`) that define the `cuvm()` shell function and install the cd-autoload hook.

**Gate (spike → unit):** shim-protocol. Black-box proof: under `bash --norc` / `zsh -f`, sourcing the shim + `cd` into a `.cuda-version` dir mutates `$CUDA_HOME`, does not duplicate PATH on repeated activation, and reverts to the `default` alias on leaving the pinned dir (mirrors nvm `load-nvmrc`).

**Contract notes (do not re-derive — spec §5, §8, §2.5):**
- The binary prints **only** shell code to **stdout**; diagnostics go to **stderr**. The shim `eval`s it. This is the whole "print-then-eval" protocol.
- The outer shim function passes `use|env|shell|default` through `eval "$(command cuvm "$@" --shell bash)"`; everything else is a plain `command cuvm "$@"` passthrough.
- `hook` emits the cd-autoload glue: bash chains `PROMPT_COMMAND`; zsh uses `add-zsh-hook chpwd`. On leaving a pinned dir, revert to the `default` alias.
- `.cuda-version` is discovered by **upward walk** (already implemented by `Resolver::find_pin_upward` in WU-2; the hook just calls `cuvm use` / `cuvm use default` and lets the resolver do the walk).
- `CUVM_INJECTED` breadcrumb cleanup (no PATH dup) is already produced by the WU-5 `Activator::emit_env` output; WU-6 only proves it end-to-end through the shim.

---

#### Task 6.1 — Hidden `cuvm hook --shell <s>` subcommand

**Files:**
- Modify: `crates/cuvm-cli/src/cli.rs` (add `Hook` variant + `--shell` arg)
- Create: `crates/cuvm-cli/src/commands/hook.rs`
- Modify: `crates/cuvm-cli/src/commands/mod.rs` (declare `pub mod hook;`)
- Modify: `crates/cuvm-cli/Cargo.toml` (add dev-deps `insta`, `assert_cmd`, `predicates`)
- Create: `crates/cuvm-cli/tests/hook_golden.rs`

The `hook` subcommand is a thin adapter: parse `--shell`, look up the Unix `Activator` from the runtime factory, call `Activator::hook(sh)` (WU-5), print the returned string to stdout. No resolution, no fs.

1. - [ ] **Step:** Add `insta`, `assert_cmd`, `predicates` to `cuvm-cli` dev-deps so the golden + e2e tests compile.

   ```toml
   # crates/cuvm-cli/Cargo.toml  (append under [dev-dependencies])
   [dev-dependencies]
   insta = "1"
   assert_cmd = "2"
   predicates = "3"
   ```

2. - [ ] **Step:** Write the failing golden test for `cuvm hook --shell bash` and `--shell zsh`.

   ```rust
   // crates/cuvm-cli/tests/hook_golden.rs
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
   ```

3. - [ ] **Step:** Run it, see it fail.

   `cargo test -p cuvm-cli --test hook_golden`

   Expected: fail — compile error `no variant Hook` / `error: unrecognized subcommand 'hook'` (the binary exits non-zero, the `assert!` on `out.status.success()` fires).

4. - [ ] **Step:** Add the `Hook` subcommand to the clap tree (hidden from help). The `Shell` parse uses clap's `ValueEnum` over `cuvm_core::Shell`.

   ```rust
   // crates/cuvm-cli/src/cli.rs
   use clap::{Parser, Subcommand, ValueEnum};
   use cuvm_core::Shell;

   #[derive(Parser, Debug)]
   #[command(name = "cuvm", version, about = "CUDA toolkit version manager")]
   pub struct Cli {
       #[command(subcommand)]
       pub command: Command,
   }

   /// clap-facing mirror of cuvm_core::Shell (keeps the ValueEnum derive out of core).
   #[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
   pub enum ShellArg {
       Bash,
       Zsh,
       #[value(name = "powershell")]
       PowerShell,
       Cmd,
   }

   impl From<ShellArg> for Shell {
       fn from(s: ShellArg) -> Self {
           match s {
               ShellArg::Bash => Shell::Bash,
               ShellArg::Zsh => Shell::Zsh,
               ShellArg::PowerShell => Shell::PowerShell,
               ShellArg::Cmd => Shell::Cmd,
           }
       }
   }

   #[derive(Subcommand, Debug)]
   pub enum Command {
       /// Print cd-autoload hook glue for the given shell (shim-only).
       #[command(hide = true)]
       Hook {
           #[arg(long, value_enum)]
           shell: ShellArg,
       },
   }
   ```

5. - [ ] **Step:** Implement the `hook` command handler against the runtime `Activator`.

   ```rust
   // crates/cuvm-cli/src/commands/hook.rs
   use anyhow::Result;
   use cuvm_core::{Os, Shell};

   /// `cuvm hook --shell <s>` — emit the cd-autoload glue to stdout.
   ///
   /// Pure adapter: dispatch to the runtime Activator (WU-5 impl) and print.
   pub fn run(shell: Shell) -> Result<()> {
       let activator = cuvm_platform::new_activator(Os::Linux);
       let script = activator.hook(shell)?;
       print!("{script}");
       Ok(())
   }
   ```

   > Note: emission backend dispatch is **runtime** (spec §3). `cuvm hook` always targets the Unix `Activator` here because WU-6 owns the Unix lane; the Windows `hook` wiring is WU-9. `new_activator(Os::Linux)` returns the Unix impl on every host so this command and its golden test run on Linux CI.

6. - [ ] **Step:** Declare the module and route the subcommand in the composition root.

   ```rust
   // crates/cuvm-cli/src/commands/mod.rs
   pub mod hook;
   ```

   ```rust
   // crates/cuvm-cli/src/main.rs  (inside the match on cli.command)
   use crate::cli::{Cli, Command};
   use clap::Parser;

   fn main() -> anyhow::Result<()> {
       let cli = Cli::parse();
       match cli.command {
           Command::Hook { shell } => commands::hook::run(shell.into()),
       }
   }
   ```

7. - [ ] **Step:** Run the test, accept the snapshot, see it pass.

   `cargo test -p cuvm-cli --test hook_golden` then `cargo insta accept`

   Expected: first run reports new snapshots `hook_bash` / `hook_zsh`; after `cargo insta accept`, re-run is `test result: ok. 2 passed`.

8. - [ ] **Step:** Verify the accepted snapshots match the WU-5 contract (bash chains `PROMPT_COMMAND`, zsh uses `add-zsh-hook chpwd`).

   `cargo test -p cuvm-cli --test hook_golden`

   Expected: `ok. 2 passed`. Committed snapshot `crates/cuvm-cli/tests/snapshots/hook_golden__hook_bash.snap` contains a `PROMPT_COMMAND="__cuvm_autoload${PROMPT_COMMAND:+;$PROMPT_COMMAND}"`-style chain; `..__hook_zsh.snap` contains `autoload -Uz add-zsh-hook` + `add-zsh-hook chpwd __cuvm_autoload`.

9. - [ ] **Step:** Commit.

   ```bash
   git add crates/cuvm-cli/Cargo.toml crates/cuvm-cli/src/cli.rs crates/cuvm-cli/src/commands/mod.rs crates/cuvm-cli/src/commands/hook.rs crates/cuvm-cli/src/main.rs crates/cuvm-cli/tests/hook_golden.rs crates/cuvm-cli/tests/snapshots/ \
     && git commit -m "feat(cli): hidden 'cuvm hook --shell' subcommand with golden snapshots

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
   ```

---

#### Task 6.2 — Hidden `cuvm env <spec> --shell <s>` subcommand

**Files:**
- Modify: `crates/cuvm-cli/src/cli.rs` (add `Env` variant)
- Create: `crates/cuvm-cli/src/commands/env.rs`
- Modify: `crates/cuvm-cli/src/commands/mod.rs` (declare `pub mod env;`)
- Modify: `crates/cuvm-cli/src/main.rs` (route `Env`)
- Create: `crates/cuvm-cli/tests/env_cmd.rs`

`env` resolves the spec via `Resolver` (WU-2), then renders the env script via `Activator::emit_env` (WU-5). The special spec `default` (and an empty/`.` spec → resolve from cwd) reverts via `Activator::emit_deactivate` only when no bundle resolves. The composition root constructs the concrete `Resolver` (backed by `Inventory` from WU-3) and the Unix `Activator`.

1. - [ ] **Step:** Write the failing e2e test: `cuvm env <ver> --shell bash` over a fixture `CUVM_HOME` prints an export of `CUDA_HOME` pointing at the resolved version dir, and `cuvm env default --shell bash` over an empty home prints a deactivate script.

   ```rust
   // crates/cuvm-cli/tests/env_cmd.rs
   use assert_cmd::Command;
   use assert_fs::prelude::*;
   use predicates::str::contains;

   /// Build a minimal CUVM_HOME with one adopted bundle so the Resolver finds it.
   fn seed_home() -> assert_fs::TempDir {
       let home = assert_fs::TempDir::new().unwrap();
       // versions/12.4.1 tree (adopted-style, has lib64)
       home.child("versions/12.4.1/bin").create_dir_all().unwrap();
       home.child("versions/12.4.1/lib64").create_dir_all().unwrap();
       // manifest.json registering the bundle (schema per cuvm-store / WU-3).
       home.child("manifest.json")
           .write_str(
               r#"{
   "schema_version": 1,
   "bundles": [
     { "version": "12.4.1", "source": "Adopted",
       "path": "versions/12.4.1", "cudnn": null,
       "components": ["cuda_nvcc","cuda_cudart"], "sha256": null,
       "installed_at": "2026-06-08T00:00:00Z" }
   ],
   "aliases": {},
   "pins": {},
   "last_driver": null
   }"#,
           )
           .unwrap();
       home
   }

   #[test]
   fn env_exact_spec_emits_cuda_home() {
       let home = seed_home();
       Command::cargo_bin("cuvm")
           .unwrap()
           .env("CUVM_HOME", home.path())
           .args(["env", "12.4.1", "--shell", "bash"])
           .assert()
           .success()
           .stdout(contains("export CUDA_HOME=").and(contains("versions/12.4.1")))
           .stdout(contains("export CUVM_INJECTED="));
   }

   #[test]
   fn env_default_on_empty_home_emits_deactivate() {
       let home = assert_fs::TempDir::new().unwrap();
       home.child("manifest.json")
           .write_str(
               r#"{"schema_version":1,"bundles":[],"aliases":{},"pins":{},"last_driver":null}"#,
           )
           .unwrap();
       Command::cargo_bin("cuvm")
           .unwrap()
           .env("CUVM_HOME", home.path())
           .args(["env", "default", "--shell", "bash"])
           .assert()
           .success()
           // deactivate strips the breadcrumb and unsets CUVM_CURRENT
           .stdout(contains("unset CUVM_CURRENT").or(contains("CUVM_INJECTED")));
   }
   ```

   Add `assert_fs` to dev-deps:

   ```toml
   # crates/cuvm-cli/Cargo.toml  (append under [dev-dependencies])
   assert_fs = "1"
   ```

2. - [ ] **Step:** Run it, see it fail.

   `cargo test -p cuvm-cli --test env_cmd`

   Expected: fail — `error: unrecognized subcommand 'env'` (both tests assert `.success()`, which fails on the non-zero exit).

3. - [ ] **Step:** Add the `Env` variant to the clap tree (hidden), with a positional `spec` and `--shell`.

   ```rust
   // crates/cuvm-cli/src/cli.rs   (add a variant inside `enum Command`)
   /// Print the env-mutation script for <spec> (shim-only).
   #[command(hide = true)]
   Env {
       /// Version spec: exact/minor/major/latest/alias/default, or empty for cwd.
       spec: Option<String>,
       #[arg(long, value_enum)]
       shell: ShellArg,
   },
   ```

4. - [ ] **Step:** Implement the `env` handler: resolve, then render. The composition root passes in the wired `Resolver` and `Activator` so the handler stays pure-ish.

   ```rust
   // crates/cuvm-cli/src/commands/env.rs
   use anyhow::Result;
   use cuvm_app::Resolver;
   use cuvm_core::{Os, Shell};

   /// `cuvm env <spec> --shell <s>` — resolve then emit the env script to stdout.
   ///
   /// `spec == None` -> resolve from cwd (.cuda-version upward walk, else default).
   /// `spec == "default"` with nothing resolvable -> emit a deactivate script.
   pub fn run(resolver: &dyn Resolver, spec: Option<String>, shell: Shell) -> Result<()> {
       let activator = cuvm_platform::new_activator(Os::Linux);

       let resolved = match spec.as_deref() {
           None | Some("") | Some(".") => {
               let cwd = std::env::current_dir()?;
               resolver.resolve_from_dir(&cwd)?
           }
           Some(s) => Some(resolver.resolve(s)?),
       };

       let script = match resolved {
           Some(r) => activator.emit_env(&r.bundle, shell)?,
           None => activator.emit_deactivate(shell)?,
       };
       print!("{script}");
       Ok(())
   }
   ```

5. - [ ] **Step:** Declare the module and route the subcommand, building the concrete `Resolver` in the composition root.

   ```rust
   // crates/cuvm-cli/src/commands/mod.rs
   pub mod env;
   ```

   ```rust
   // crates/cuvm-cli/src/main.rs  (extend the match; `wiring::resolver()` builds the
   // Inventory-backed Resolver from CUVM_HOME — provided by the WU-2/WU-3 composition root)
   Command::Env { spec, shell } => {
       let resolver = crate::wiring::resolver()?;
       commands::env::run(resolver.as_ref(), spec, shell.into())
   }
   ```

6. - [ ] **Step:** Run the tests, see them pass.

   `cargo test -p cuvm-cli --test env_cmd`

   Expected: `test result: ok. 2 passed`.

7. - [ ] **Step:** Commit.

   ```bash
   git add crates/cuvm-cli/Cargo.toml crates/cuvm-cli/src/cli.rs crates/cuvm-cli/src/commands/mod.rs crates/cuvm-cli/src/commands/env.rs crates/cuvm-cli/src/main.rs crates/cuvm-cli/tests/env_cmd.rs \
     && git commit -m "feat(cli): hidden 'cuvm env <spec> --shell' resolve+emit subcommand

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
   ```

---

#### Task 6.3 — Author the Unix shims (`cuvm.sh`, `cuvm.zsh`) and embed via `include_str!`

**Files:**
- Create: `shims/cuvm.sh`
- Create: `shims/cuvm.zsh`
- Create: `crates/cuvm-cli/src/shims.rs`
- Modify: `crates/cuvm-cli/src/main.rs` (declare `mod shims;`)
- Create: `crates/cuvm-cli/tests/shim_embed.rs`

The shim files define the `cuvm()` wrapper (`use|env|shell|default` → `eval "$(command cuvm "$@" --shell <sh>)"`, else passthrough) plus the `__cuvm_autoload` hook function that the `cuvm hook` output references. The CLI embeds them with `include_str!` so a future `cuvm hook` / install step can write them to `~/.cuvm/shims/`.

1. - [ ] **Step:** Write the failing test that asserts the embedded shim strings exist and contain the load-bearing protocol lines.

   ```rust
   // crates/cuvm-cli/tests/shim_embed.rs
   use cuvm_cli::shims;

   #[test]
   fn bash_shim_defines_function_and_eval_protocol() {
       let s = shims::BASH_SHIM;
       assert!(s.contains("cuvm()"), "bash shim must define cuvm() function");
       // dispatch the four mutating verbs through eval of `command cuvm ... --shell bash`
       assert!(s.contains("use|env|shell|default"));
       assert!(s.contains(r#"eval "$(command cuvm "$@" --shell bash)""#));
       // passthrough for everything else
       assert!(s.contains(r#"command cuvm "$@""#));
       // the hook function the `cuvm hook` output chains into
       assert!(s.contains("__cuvm_autoload"));
   }

   #[test]
   fn zsh_shim_uses_bash_emitter_and_defines_function() {
       let s = shims::ZSH_SHIM;
       assert!(s.contains("cuvm()"));
       // zsh emits with --shell bash too (bash/zsh env syntax is identical here)
       assert!(s.contains(r#"eval "$(command cuvm "$@" --shell bash)""#));
       assert!(s.contains("__cuvm_autoload"));
   }
   ```

2. - [ ] **Step:** Run it, see it fail.

   `cargo test -p cuvm-cli --test shim_embed`

   Expected: fail — `error[E0432]: unresolved import` / `cannot find module shims` (and the shim source files do not exist yet).

3. - [ ] **Step:** Author `shims/cuvm.sh`. The `cuvm()` function dispatches the four mutating verbs through eval; the `__cuvm_autoload` function does the cd-revert logic (calls `cuvm use` with no spec → resolver walks `.cuda-version`; on no pin, `cuvm use default`).

   ```sh
   # shims/cuvm.sh — sourced from ~/.bashrc. Defines the cuvm() wrapper + autoload hook.
   # The binary prints shell code to stdout; this function eval's it (print-then-eval).

   cuvm() {
     case "${1:-}" in
       use|env|shell|default)
         # mutating verbs: capture stdout (env script) and eval it in this shell.
         eval "$(command cuvm "$@" --shell bash)"
         ;;
       *)
         command cuvm "$@"
         ;;
     esac
   }

   # cd-autoload: re-activate from .cuda-version (upward walk done by the binary),
   # or revert to the persistent default when no pin is in scope. Tracks the last
   # directory we acted on so we only re-emit env when the pin context changes.
   __cuvm_autoload() {
     # `cuvm env` with no spec resolves from cwd (.cuda-version, else default);
     # eval applies it. Diagnostics go to stderr and are left visible.
     local _script
     _script="$(command cuvm env --shell bash 2>/dev/null)" || return 0
     [ -n "$_script" ] && eval "$_script"
   }
   ```

4. - [ ] **Step:** Author `shims/cuvm.zsh`. Same function body; zsh reuses the bash emitter (identical export syntax). Kept as a separate file so the hook glue differs (chpwd vs PROMPT_COMMAND).

   ```sh
   # shims/cuvm.zsh — sourced from ~/.zshrc. Defines the cuvm() wrapper + autoload hook.

   cuvm() {
     case "${1:-}" in
       use|env|shell|default)
         eval "$(command cuvm "$@" --shell bash)"
         ;;
       *)
         command cuvm "$@"
         ;;
     esac
   }

   __cuvm_autoload() {
     local _script
     _script="$(command cuvm env --shell bash 2>/dev/null)" || return 0
     [ -n "$_script" ] && eval "$_script"
   }
   ```

5. - [ ] **Step:** Embed both shims with `include_str!` and expose them from a library target so tests and a future install step can reach them.

   ```rust
   // crates/cuvm-cli/src/shims.rs
   //! Embedded Unix shell shims. Paths are relative to this source file.
   //! (Windows shims ps1/cmd are added in WU-9.)

   /// `cuvm.sh` — bash shim: cuvm() wrapper + __cuvm_autoload hook function.
   pub const BASH_SHIM: &str = include_str!("../../../shims/cuvm.sh");

   /// `cuvm.zsh` — zsh shim: cuvm() wrapper + __cuvm_autoload hook function.
   pub const ZSH_SHIM: &str = include_str!("../../../shims/cuvm.zsh");
   ```

   Expose `shims` from a lib target (so the integration test in `tests/` can `use cuvm_cli::shims`):

   ```rust
   // crates/cuvm-cli/src/lib.rs   (create if absent; main.rs `use cuvm_cli::*`)
   pub mod cli;
   pub mod commands;
   pub mod shims;
   pub mod wiring;
   ```

   ```toml
   # crates/cuvm-cli/Cargo.toml  — ensure both a lib and a bin target exist
   [lib]
   name = "cuvm_cli"
   path = "src/lib.rs"

   [[bin]]
   name = "cuvm"
   path = "src/main.rs"
   ```

6. - [ ] **Step:** Run the test, see it pass.

   `cargo test -p cuvm-cli --test shim_embed`

   Expected: `test result: ok. 2 passed`.

7. - [ ] **Step:** Sanity-check that the shim files are valid POSIX/zsh syntax (no eval-time syntax errors) without sourcing side effects.

   `bash -n shims/cuvm.sh && zsh -n shims/cuvm.zsh && echo SHIM_SYNTAX_OK`

   Expected: prints `SHIM_SYNTAX_OK` (both `-n` no-exec parse checks pass).

8. - [ ] **Step:** Commit.

   ```bash
   git add shims/cuvm.sh shims/cuvm.zsh crates/cuvm-cli/src/shims.rs crates/cuvm-cli/src/lib.rs crates/cuvm-cli/Cargo.toml crates/cuvm-cli/tests/shim_embed.rs \
     && git commit -m "feat(cli): author and embed Unix cuvm.sh/cuvm.zsh shims via include_str!

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
   ```

---

#### Task 6.4 — Black-box shell integration test (bash `--norc`, zsh `-f`)

**Files:**
- Create: `crates/cuvm-cli/tests/fixtures/run_shim_bash.sh`
- Create: `crates/cuvm-cli/tests/fixtures/run_shim_zsh.sh`
- Create: `crates/cuvm-cli/tests/shim_blackbox.rs`

This is the gate proof. A driver script sources the shim under a clean shell, `cd`s into a fixture dir holding `.cuda-version`, runs the autoload hook, and asserts: (a) `$CUDA_HOME` changed to the pinned version dir; (b) re-running activation does not duplicate the PATH segment; (c) leaving the pinned dir reverts to the `default` alias. The Rust test builds the `cuvm` binary, seeds a `CUVM_HOME`, and shells out to the driver under `bash --norc` / `zsh -f`.

1. - [ ] **Step:** Write the bash driver fixture. It sources the shim, exercises the autoload + dup-check + revert, and echoes machine-checkable lines.

   ```sh
   # crates/cuvm-cli/tests/fixtures/run_shim_bash.sh
   # Args: $1 = path to cuvm binary dir (prepended to PATH), $2 = CUVM_HOME,
   #       $3 = path to cuvm.sh shim, $4 = pinned fixture dir, $5 = unpinned dir.
   set -eu
   export PATH="$1:$PATH"
   export CUVM_HOME="$2"
   # shellcheck disable=SC1090
   . "$3"

   # 1) enter pinned dir -> autoload activates the pinned toolkit
   cd "$4"
   __cuvm_autoload
   echo "CUDA_HOME_AFTER_PIN=$CUDA_HOME"
   echo "CURRENT_AFTER_PIN=${CUVM_CURRENT:-}"

   # 2) re-activate -> PATH must not gain a second copy of the injected bin
   __cuvm_autoload
   _dups="$(printf '%s' "$PATH" | tr ':' '\n' | grep -c "$CUDA_HOME/bin" || true)"
   echo "PATH_BIN_COUNT=$_dups"

   # 3) leave pinned dir -> revert to the default alias
   cd "$5"
   __cuvm_autoload
   echo "CURRENT_AFTER_LEAVE=${CUVM_CURRENT:-}"
   ```

2. - [ ] **Step:** Write the zsh driver fixture (same protocol; zsh-clean shell).

   ```sh
   # crates/cuvm-cli/tests/fixtures/run_shim_zsh.sh
   # Args identical to the bash driver; $3 = path to cuvm.zsh shim.
   set -eu
   export PATH="$1:$PATH"
   export CUVM_HOME="$2"
   source "$3"

   cd "$4"
   __cuvm_autoload
   echo "CUDA_HOME_AFTER_PIN=$CUDA_HOME"
   echo "CURRENT_AFTER_PIN=${CUVM_CURRENT:-}"

   __cuvm_autoload
   _dups="$(printf '%s' "$PATH" | tr ':' '\n' | grep -c "$CUDA_HOME/bin" || true)"
   echo "PATH_BIN_COUNT=$_dups"

   cd "$5"
   __cuvm_autoload
   echo "CURRENT_AFTER_LEAVE=${CUVM_CURRENT:-}"
   ```

3. - [ ] **Step:** Write the failing Rust black-box test that seeds a home with two bundles (a pinned `12.4.1` and a `default` alias → `12.6.0`), a pinned dir with `.cuda-version`, and an unpinned dir, then runs both drivers and parses the echoed lines.

   ```rust
   // crates/cuvm-cli/tests/shim_blackbox.rs
   use assert_cmd::cargo::cargo_bin;
   use assert_fs::prelude::*;
   use std::collections::HashMap;
   use std::process::Command;

   /// Seed CUVM_HOME with two adopted bundles + a `default` alias -> 12.6.0.
   fn seed_home() -> assert_fs::TempDir {
       let home = assert_fs::TempDir::new().unwrap();
       for v in ["12.4.1", "12.6.0"] {
           home.child(format!("versions/{v}/bin")).create_dir_all().unwrap();
           home.child(format!("versions/{v}/lib64")).create_dir_all().unwrap();
       }
       home.child("manifest.json")
           .write_str(
               r#"{
   "schema_version": 1,
   "bundles": [
     {"version":"12.4.1","source":"Adopted","path":"versions/12.4.1","cudnn":null,
      "components":["cuda_nvcc"],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"},
     {"version":"12.6.0","source":"Adopted","path":"versions/12.6.0","cudnn":null,
      "components":["cuda_nvcc"],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}
   ],
   "aliases": {"default":"12.6.0"},
   "pins": {},
   "last_driver": null
   }"#,
           )
           .unwrap();
       home
   }

   fn parse_lines(stdout: &str) -> HashMap<String, String> {
       stdout
           .lines()
           .filter_map(|l| l.split_once('='))
           .map(|(k, v)| (k.to_string(), v.to_string()))
           .collect()
   }

   /// Run a shell driver under a clean shell; return parsed key=value output.
   fn run_driver(shell: &str, clean_flag: &str, driver: &str, shim: &str) -> HashMap<String, String> {
       let home = seed_home();
       let pinned = assert_fs::TempDir::new().unwrap();
       pinned.child(".cuda-version").write_str("12.4.1\n").unwrap();
       let unpinned = assert_fs::TempDir::new().unwrap();

       let bin = cargo_bin("cuvm");
       let bin_dir = bin.parent().unwrap();

       let out = Command::new(shell)
           .arg(clean_flag) // bash: --norc ; zsh: -f
           .arg(driver)
           .arg(bin_dir)
           .arg(home.path())
           .arg(shim)
           .arg(pinned.path())
           .arg(unpinned.path())
           .output()
           .unwrap();
       assert!(
           out.status.success(),
           "{shell} driver failed; stderr: {}",
           String::from_utf8_lossy(&out.stderr)
       );
       parse_lines(&String::from_utf8(out.stdout).unwrap())
   }

   fn manifest_dir(rel: &str) -> &str {
       env!("CARGO_MANIFEST_DIR").leak_helper(rel)
   }
   trait LeakHelper { fn leak_helper(&'static self, rel: &str) -> &'static str; }
   impl LeakHelper for str {
       fn leak_helper(&'static self, rel: &str) -> &'static str {
           Box::leak(format!("{self}/{rel}").into_boxed_str())
       }
   }

   #[test]
   fn bash_shim_activates_dedups_and_reverts() {
       let driver = manifest_dir("tests/fixtures/run_shim_bash.sh");
       let shim = manifest_dir("../../shims/cuvm.sh");
       let m = run_driver("bash", "--norc", driver, shim);

       assert!(m["CUDA_HOME_AFTER_PIN"].ends_with("versions/12.4.1"),
           "pinned dir must activate 12.4.1, got {}", m["CUDA_HOME_AFTER_PIN"]);
       assert_eq!(m["CURRENT_AFTER_PIN"], "12.4.1");
       assert_eq!(m["PATH_BIN_COUNT"], "1", "no PATH duplication on re-activation");
       assert_eq!(m["CURRENT_AFTER_LEAVE"], "12.6.0", "leaving pinned dir reverts to default");
   }

   #[test]
   fn zsh_shim_activates_dedups_and_reverts() {
       let driver = manifest_dir("tests/fixtures/run_shim_zsh.sh");
       let shim = manifest_dir("../../shims/cuvm.zsh");
       let m = run_driver("zsh", "-f", driver, shim);

       assert!(m["CUDA_HOME_AFTER_PIN"].ends_with("versions/12.4.1"));
       assert_eq!(m["CURRENT_AFTER_PIN"], "12.4.1");
       assert_eq!(m["PATH_BIN_COUNT"], "1");
       assert_eq!(m["CURRENT_AFTER_LEAVE"], "12.6.0");
   }
   ```

   > Note: this lane requires `bash` and `zsh` on the runner; in CI it runs on the dedicated shell lane (`apt-get install -y zsh`). The `LeakHelper` shim just joins `CARGO_MANIFEST_DIR` with a relative path into a `'static str` for the `Command` args; replace with a `PathBuf` local if you prefer (functionally identical).

4. - [ ] **Step:** Run it, see it fail.

   `cargo test -p cuvm-cli --test shim_blackbox`

   Expected: fail. The first failure surfaces because `cuvm env` (no spec) must resolve from cwd via `.cuda-version` and revert to `default` on leave — assert the exact gap, e.g. `CURRENT_AFTER_LEAVE` is empty (no revert) or `CUDA_HOME_AFTER_PIN` does not end with `versions/12.4.1`. Confirm the assertion message names the missing behavior before fixing.

5. - [ ] **Step:** Make the revert behavior correct. `cuvm env` with no resolvable pin must fall through to the `default` alias rather than emitting a bare deactivate, so leaving a pinned dir restores the default toolkit (mirrors nvm `load-nvmrc`). Adjust the `env` handler's cwd branch to prefer `default` when `resolve_from_dir` yields `None` but a `default` alias exists.

   ```rust
   // crates/cuvm-cli/src/commands/env.rs   (replace the `None | Some("") | Some(".")` arm body)
       let resolved = match spec.as_deref() {
           None | Some("") | Some(".") => {
               let cwd = std::env::current_dir()?;
               match resolver.resolve_from_dir(&cwd)? {
                   Some(r) => Some(r),
                   // No .cuda-version in scope: fall back to the persistent default
                   // so leaving a pinned dir reverts (nvm load-nvmrc behavior).
                   None => match resolver.resolve("default") {
                       Ok(r) => Some(r),
                       Err(_) => None, // no default set -> deactivate
                   },
               }
           }
           Some(s) => Some(resolver.resolve(s)?),
       };
   ```

   > `resolve_from_dir` already returns the pinned bundle when `.cuda-version` is found via upward walk (WU-2), and `None` otherwise. The added `default` fallback is the revert path. If `Resolver::resolve_from_dir` already encodes "else default" per its WU-2 doc comment (`// .cuda-version, else default`), this fallback is a no-op safety net and the test still passes — keep it for an explicit, testable contract at the CLI seam.

6. - [ ] **Step:** Run the black-box tests, see them pass.

   `cargo test -p cuvm-cli --test shim_blackbox`

   Expected: `test result: ok. 2 passed` — both bash and zsh: `CUDA_HOME` switched to `versions/12.4.1`, `PATH_BIN_COUNT=1` (no dup), `CURRENT_AFTER_LEAVE=12.6.0` (reverted to default).

7. - [ ] **Step:** Run the full crate suite to confirm no regression across env/hook/shim tasks.

   `cargo test -p cuvm-cli`

   Expected: all of `hook_golden`, `env_cmd`, `shim_embed`, `shim_blackbox` green; `test result: ok` for each.

8. - [ ] **Step:** Commit.

   ```bash
   git add crates/cuvm-cli/tests/fixtures/run_shim_bash.sh crates/cuvm-cli/tests/fixtures/run_shim_zsh.sh crates/cuvm-cli/tests/shim_blackbox.rs crates/cuvm-cli/src/commands/env.rs \
     && git commit -m "test(cli): black-box shell shim test (bash --norc / zsh -f); revert-to-default on dir leave

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
   ```

---

**WU-6 done when:** `cargo test -p cuvm-cli` is green; `cuvm hook --shell bash|zsh` golden snapshots are committed; the black-box shell test proves activate-on-cd, no-PATH-dup on re-activate, and revert-to-default on leave under `bash --norc` and `zsh -f`. Gate **shim-protocol** satisfied. Windows shim/hook (ps1/cmd, chained `prompt()`) is out of scope here and lands in WU-9.

---

### WU-7: Compat engine + embedded data tables + nvidia-smi probe

**Depends on:** WU-0 (workspace, `cuvm-core`/`cuvm-nvidia`/`cuvm-app` crates exist with `Cargo.toml`s and `serde`/`serde_json`/`anyhow`/`thiserror`/`time` declared in `[workspace.dependencies]`), WU-1 (`cuvm-app` declares the `CompatEngine` and `DriverProbe` trait ports; `Verdict`/`Severity` live in `cuvm-app`). WU-2 supplies `Version` with numeric tuple `Ord` and `Version::parse`, plus `Platform`/`Os`/`Arch`/`GpuClass`/`Driver`. This WU implements the concrete `DefaultCompatEngine` in `cuvm-core` over embedded JSON tables, and the `nvidia-smi` `SmiProbe` in `cuvm-nvidia`.

**Ground-truth source:** spec §2.4 and §12. Treat the driver-minimum table, the Windows-N/A-from-13.0 correction, the cuDNN major rules, and the minor-version floors as FACTS — do not re-derive. All comparisons are numeric tuple compares via `Version`'s `Ord`, NEVER lexical string compares.

This WU has four tasks:
1. Embedded `driver_ceiling.json` table + typed parse (`cuvm-core/data` + `tables.rs`).
2. Embedded `cudnn_matrix.json` table + typed parse.
3. `DefaultCompatEngine`: `max_toolkit_for_driver`, `check_toolkit`, `pair_cudnn`, `validate_pair`.
4. `cuvm-nvidia` `SmiProbe`: parse `nvidia-smi` driver version, graceful-absent.

---

#### Task 7.1 — Embedded driver-ceiling table + typed parse

**Files:**
- Create: `crates/cuvm-core/data/driver_ceiling.json`
- Create: `crates/cuvm-core/src/compat/mod.rs`
- Create: `crates/cuvm-core/src/compat/tables.rs`
- Modify: `crates/cuvm-core/src/lib.rs` (add `pub mod compat;`)
- Modify: `crates/cuvm-core/Cargo.toml` (ensure `serde` derive + `serde_json` present)
- Test: inline `#[cfg(test)]` in `crates/cuvm-core/src/compat/tables.rs`

The table encodes spec §12 verbatim. `windows_min` is `null` for all 13.x (the load-bearing correction: Windows display driver unbundled at **13.0**, so all of 13.x is N/A — not 13.1). There is **no 12.7 row** (NVIDIA skipped it). Driver strings are full dotted strings parsed into `Version` tuples at load.

Steps:

- [ ] **Step 1 — Write the embedded data file.** Create `crates/cuvm-core/data/driver_ceiling.json` with the exact §12 rows (GA rows). Note `null` windows_min for every 13.x row and no 12.7 entry:

```json
{
  "schema_version": 1,
  "snapshot": "CUDA 13.3, mid-2026",
  "rows": [
    { "cuda": "11.8", "linux_min": "520.61.05", "windows_min": "520.06" },
    { "cuda": "12.0", "linux_min": "525.60.13", "windows_min": "527.41" },
    { "cuda": "12.1", "linux_min": "530.30.02", "windows_min": "531.14" },
    { "cuda": "12.2", "linux_min": "535.54.03", "windows_min": "536.25" },
    { "cuda": "12.3", "linux_min": "545.23.06", "windows_min": "545.84" },
    { "cuda": "12.4", "linux_min": "550.54.14", "windows_min": "551.61" },
    { "cuda": "12.5", "linux_min": "555.42.02", "windows_min": "555.85" },
    { "cuda": "12.6", "linux_min": "560.28.03", "windows_min": "560.76" },
    { "cuda": "12.8", "linux_min": "570.26", "windows_min": "570.65" },
    { "cuda": "12.9", "linux_min": "575.51.03", "windows_min": "576.02" },
    { "cuda": "13.0", "linux_min": "580.65.06", "windows_min": null },
    { "cuda": "13.1", "linux_min": "590.44.01", "windows_min": null },
    { "cuda": "13.2", "linux_min": "595.45.04", "windows_min": null },
    { "cuda": "13.3", "linux_min": "610.43.02", "windows_min": null }
  ]
}
```

- [ ] **Step 2 — Write the failing test for table parse + the Windows-13.0 regression.** Create `crates/cuvm-core/src/compat/tables.rs` containing only the test module first (so it fails to compile/find symbols), then the impl will be added in Step 4. Put this at the top of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::Version;

    #[test]
    fn loads_all_driver_rows_with_no_127() {
        let t = DriverCeilingTable::load();
        // 14 GA rows from spec §12; 12.7 deliberately absent.
        assert_eq!(t.rows.len(), 14);
        assert!(
            !t.rows.iter().any(|r| r.cuda == Version::parse("12.7").unwrap()),
            "CUDA 12.7 must not exist (NVIDIA skipped it)"
        );
    }

    #[test]
    fn driver_strings_parse_as_numeric_tuples_not_lexical() {
        let t = DriverCeilingTable::load();
        let r128 = t.row_for(&Version::parse("12.8").unwrap()).unwrap();
        let r129 = t.row_for(&Version::parse("12.9").unwrap()).unwrap();
        // 570.26 < 575.51.03 numerically; lexically "570.26" < "575..." too,
        // but 570.26 vs 570.124.06 is the real trap — assert tuple compare holds.
        assert!(r128.linux_min < r129.linux_min);
        assert!(Version::parse("570.26").unwrap() < Version::parse("570.124.06").unwrap());
    }

    #[test]
    fn windows_na_begins_at_cuda_13_0_not_13_1() {
        let t = DriverCeilingTable::load();
        // 12.9 still has a Windows minimum.
        assert!(t.row_for(&Version::parse("12.9").unwrap()).unwrap().windows_min.is_some());
        // CRITICAL regression (spec §2.4): all of 13.x is Windows N/A, starting at 13.0.
        for v in ["13.0", "13.1", "13.2", "13.3"] {
            let row = t.row_for(&Version::parse(v).unwrap()).unwrap();
            assert!(
                row.windows_min.is_none(),
                "CUDA {v} must be Windows N/A (driver unbundled at 13.0)"
            );
        }
    }

    #[test]
    fn linux_min_is_present_for_every_row() {
        let t = DriverCeilingTable::load();
        for r in &t.rows {
            // linux_min is non-optional; this just exercises the field exists & parsed.
            assert!(r.linux_min >= Version::parse("520.0").unwrap());
        }
    }
}
```

- [ ] **Step 3 — Run it, see it fail.** `cargo test -p cuvm-core compat::tables::` — Expected: **fail** with `error[E0433]: failed to resolve: use of undeclared type 'DriverCeilingTable'` (the impl does not exist yet). Also add `pub mod compat;` to `crates/cuvm-core/src/lib.rs` and `pub mod tables;` to a new `crates/cuvm-core/src/compat/mod.rs` so the module is reachable; without those the test target won't compile, which is the expected red.

  `crates/cuvm-core/src/compat/mod.rs` (minimal, so the test compiles to the right error):
```rust
pub mod tables;
```
  Add to `crates/cuvm-core/src/lib.rs`:
```rust
pub mod compat;
```

- [ ] **Step 4 — Minimal implementation: typed table + loader.** Prepend the impl above the test module in `crates/cuvm-core/src/compat/tables.rs`:

```rust
//! Embedded CUDA driver-minimum compatibility table (spec §12).
//!
//! Source of truth: CUDA Toolkit Release Notes "Table 3". Encoded as data with
//! separate Linux/Windows columns. ALL comparisons use `Version`'s numeric tuple
//! `Ord` — never lexical string compares.

use crate::version::Version;
use serde::Deserialize;

/// Raw JSON shape of one row in `data/driver_ceiling.json`.
#[derive(Debug, Deserialize)]
struct RawDriverRow {
    cuda: String,
    linux_min: String,
    windows_min: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawDriverTable {
    rows: Vec<RawDriverRow>,
}

/// One parsed row: a CUDA release and its per-OS minimum driver.
/// `windows_min == None` means Windows N/A (e.g. all of CUDA 13.x).
#[derive(Debug, Clone)]
pub struct DriverRow {
    pub cuda: Version,
    pub linux_min: Version,
    pub windows_min: Option<Version>,
}

/// The full embedded driver-ceiling table.
#[derive(Debug, Clone)]
pub struct DriverCeilingTable {
    pub rows: Vec<DriverRow>,
}

/// Embedded at compile time — keeps `cuvm-core` I/O-free (spec §3).
const DRIVER_CEILING_JSON: &str = include_str!("../../data/driver_ceiling.json");

impl DriverCeilingTable {
    /// Parse the embedded table. Panics only on a corrupt embedded asset, which
    /// is a build-time bug, not a runtime condition.
    pub fn load() -> Self {
        let raw: RawDriverTable = serde_json::from_str(DRIVER_CEILING_JSON)
            .expect("embedded driver_ceiling.json is valid JSON");
        let rows = raw
            .rows
            .into_iter()
            .map(|r| DriverRow {
                cuda: Version::parse(&r.cuda)
                    .expect("embedded driver_ceiling.json: cuda field parses"),
                linux_min: Version::parse(&r.linux_min)
                    .expect("embedded driver_ceiling.json: linux_min field parses"),
                windows_min: r.windows_min.as_deref().map(|s| {
                    Version::parse(s)
                        .expect("embedded driver_ceiling.json: windows_min field parses")
                }),
            })
            .collect();
        DriverCeilingTable { rows }
    }

    /// Find the row for an exact CUDA major.minor (e.g. `12.4`).
    pub fn row_for(&self, cuda: &Version) -> Option<&DriverRow> {
        self.rows.iter().find(|r| r.cuda == *cuda)
    }
}
```

- [ ] **Step 5 — Run tests, see pass.** `cargo test -p cuvm-core compat::tables::` — Expected: `test result: ok. 4 passed; 0 failed`.

- [ ] **Step 6 — Commit.**
```bash
git add crates/cuvm-core/data/driver_ceiling.json crates/cuvm-core/src/compat/mod.rs crates/cuvm-core/src/compat/tables.rs crates/cuvm-core/src/lib.rs crates/cuvm-core/Cargo.toml && git commit -m "feat(core): embed driver-ceiling compat table with Windows-13.0-NA regression

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 7.2 — Embedded cuDNN ↔ CUDA matrix + typed parse

**Files:**
- Create: `crates/cuvm-core/data/cudnn_matrix.json`
- Modify: `crates/cuvm-core/src/compat/mod.rs` (no new file; extend `tables.rs`)
- Test: inline `#[cfg(test)]` in `crates/cuvm-core/src/compat/tables.rs`

Encodes spec §12: `8.9.7 → [11,12]` (last 8.x), `9.23.0 → [12,13]` (dropped 11.x). The rule the engine enforces (Task 7.3): CUDA 13.x requires cuDNN 9.x; CUDA 11.x requires cuDNN 8.x.

Steps:

- [ ] **Step 1 — Write the embedded data file.** Create `crates/cuvm-core/data/cudnn_matrix.json`:

```json
{
  "schema_version": 1,
  "snapshot": "cuDNN 9.23.0, mid-2026",
  "entries": [
    { "cudnn": "8.9.7", "cuda_majors": [11, 12] },
    { "cudnn": "9.23.0", "cuda_majors": [12, 13] }
  ]
}
```

- [ ] **Step 2 — Write the failing test.** Append to the `#[cfg(test)] mod tests` block in `crates/cuvm-core/src/compat/tables.rs`:

```rust
    #[test]
    fn cudnn_matrix_loads_both_lines() {
        let m = CudnnMatrix::load();
        assert_eq!(m.entries.len(), 2);
        let last8 = m.entry_for(&Version::parse("8.9.7").unwrap()).unwrap();
        assert_eq!(last8.cuda_majors, vec![11, 12]);
        let nine = m.entry_for(&Version::parse("9.23.0").unwrap()).unwrap();
        assert_eq!(nine.cuda_majors, vec![12, 13]);
    }

    #[test]
    fn cudnn_matrix_maps_cuda_major_to_lines() {
        let m = CudnnMatrix::load();
        // CUDA 13 -> only the 9.x line supports it.
        let for13: Vec<u32> = m.cudnn_lines_for_cuda_major(13).iter().map(|v| v.major()).collect();
        assert_eq!(for13, vec![9]);
        // CUDA 11 -> only the 8.x line.
        let for11: Vec<u32> = m.cudnn_lines_for_cuda_major(11).iter().map(|v| v.major()).collect();
        assert_eq!(for11, vec![8]);
        // CUDA 12 -> both lines support it.
        let mut for12: Vec<u32> = m.cudnn_lines_for_cuda_major(12).iter().map(|v| v.major()).collect();
        for12.sort();
        assert_eq!(for12, vec![8, 9]);
    }
```

- [ ] **Step 3 — Run it, see it fail.** `cargo test -p cuvm-core compat::tables::cudnn` — Expected: **fail** with `error[E0433]: failed to resolve: use of undeclared type 'CudnnMatrix'`.

- [ ] **Step 4 — Minimal implementation.** Append to `crates/cuvm-core/src/compat/tables.rs` (below the driver-table impl, above the test module):

```rust
/// Raw JSON shape of one cuDNN matrix entry in `data/cudnn_matrix.json`.
#[derive(Debug, Deserialize)]
struct RawCudnnEntry {
    cudnn: String,
    cuda_majors: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct RawCudnnMatrix {
    entries: Vec<RawCudnnEntry>,
}

/// One parsed cuDNN line and the CUDA majors it supports (spec §12).
#[derive(Debug, Clone)]
pub struct CudnnEntry {
    pub cudnn: Version,
    pub cuda_majors: Vec<u32>,
}

/// The full embedded cuDNN ↔ CUDA matrix.
#[derive(Debug, Clone)]
pub struct CudnnMatrix {
    pub entries: Vec<CudnnEntry>,
}

const CUDNN_MATRIX_JSON: &str = include_str!("../../data/cudnn_matrix.json");

impl CudnnMatrix {
    pub fn load() -> Self {
        let raw: RawCudnnMatrix = serde_json::from_str(CUDNN_MATRIX_JSON)
            .expect("embedded cudnn_matrix.json is valid JSON");
        let entries = raw
            .entries
            .into_iter()
            .map(|e| CudnnEntry {
                cudnn: Version::parse(&e.cudnn)
                    .expect("embedded cudnn_matrix.json: cudnn field parses"),
                cuda_majors: e.cuda_majors,
            })
            .collect();
        CudnnMatrix { entries }
    }

    /// Find the matrix entry for an exact cuDNN version (e.g. `9.23.0`).
    pub fn entry_for(&self, cudnn: &Version) -> Option<&CudnnEntry> {
        self.entries.iter().find(|e| e.cudnn == *cudnn)
    }

    /// All cuDNN line representatives whose support set includes this CUDA major.
    pub fn cudnn_lines_for_cuda_major(&self, cuda_major: u32) -> Vec<Version> {
        self.entries
            .iter()
            .filter(|e| e.cuda_majors.contains(&cuda_major))
            .map(|e| e.cudnn.clone())
            .collect()
    }
}
```

- [ ] **Step 5 — Run tests, see pass.** `cargo test -p cuvm-core compat::tables::` — Expected: `test result: ok. 6 passed; 0 failed`.

- [ ] **Step 6 — Commit.**
```bash
git add crates/cuvm-core/data/cudnn_matrix.json crates/cuvm-core/src/compat/tables.rs && git commit -m "feat(core): embed cuDNN-CUDA compat matrix (13.x->9.x, 11.x->8.x)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 7.3 — `DefaultCompatEngine` implementing the `CompatEngine` port

**Files:**
- Create: `crates/cuvm-core/src/compat/engine.rs`
- Modify: `crates/cuvm-core/src/compat/mod.rs` (add `pub mod engine;` + re-export)
- Test: inline `#[cfg(test)]` in `crates/cuvm-core/src/compat/engine.rs`

`DefaultCompatEngine` lives in `cuvm-core` (pure logic over embedded data). The `CompatEngine` **trait** is declared in `cuvm-app` (per CONTRACT §5), and `Verdict`/`Severity` live in `cuvm-app`. To keep `cuvm-core`'s zero-internal-dep rule intact, `cuvm-core` defines its **own** result type `CompatOutcome` (severity + reason + forward_compat flag); the `cuvm-app` `impl CompatEngine for DefaultCompatEngine` (a thin adapter mapping `CompatOutcome` → `app::Verdict`) is added when WU-8 wires the CLI. This keeps the algorithm and its tests entirely inside `cuvm-core`.

Behaviors (spec §2.4 / §11):
- `max_toolkit_for_driver`: inverse lookup — highest CUDA whose per-OS minimum ≤ driver. Uses `linux_min` on Linux, `windows_min` on Windows (skip N/A rows).
- `check_toolkit(driver, want, strict)`: strict → use the exact per-release minimum (Block below it). Non-strict ("likely") → use the minor-version floors (525.60.13 for all 12.x, 580.65.06 for all 13.x): Warn (not Block) below the strict min but above the floor.
- `pair_cudnn(toolkit, available)`: pick newest available cuDNN whose line supports `toolkit.major()`.
- `validate_pair(toolkit, cudnn)`: Block if the cuDNN line does not support the toolkit's CUDA major (encodes "13.x requires 9.x, 11.x requires 8.x").

Steps:

- [ ] **Step 1 — Write the failing tests (the regression suite).** Create `crates/cuvm-core/src/compat/engine.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Arch, Os, Platform};
    use crate::version::Version;
    use crate::{Driver, GpuClass};

    fn linux_driver(ver: &str, class: GpuClass) -> Driver {
        Driver {
            present: true,
            version: Version::parse(ver).unwrap(),
            platform: Platform { os: Os::Linux, arch: Arch::X86_64 },
            gpu_class: class,
        }
    }
    fn windows_driver(ver: &str) -> Driver {
        Driver {
            present: true,
            version: Version::parse(ver).unwrap(),
            platform: Platform { os: Os::Windows, arch: Arch::X86_64 },
            gpu_class: GpuClass::GeForce,
        }
    }

    #[test]
    fn ceiling_linux_550_54_14_is_12_4() {
        let e = DefaultCompatEngine::new();
        let d = linux_driver("550.54.14", GpuClass::GeForce);
        assert_eq!(e.max_toolkit_for_driver(&d).unwrap(), Version::parse("12.4").unwrap());
    }

    #[test]
    fn ceiling_linux_565_is_12_6() {
        let e = DefaultCompatEngine::new();
        // 12.6 min = 560.28.03 (<=565), 12.8 min = 570.26 (>565) -> ceiling 12.6.
        let d = linux_driver("565.57.01", GpuClass::GeForce);
        assert_eq!(e.max_toolkit_for_driver(&d).unwrap(), Version::parse("12.6").unwrap());
    }

    #[test]
    fn ceiling_linux_552_is_12_4() {
        let e = DefaultCompatEngine::new();
        // 12.4 min = 550.54.14 (<=552), 12.5 min = 555.42.02 (>552) -> ceiling 12.4.
        let d = linux_driver("552.12", GpuClass::GeForce);
        assert_eq!(e.max_toolkit_for_driver(&d).unwrap(), Version::parse("12.4").unwrap());
    }

    #[test]
    fn ceiling_uses_numeric_tuple_compare_not_lexical() {
        let e = DefaultCompatEngine::new();
        // 570.26 < 570.124.06 numerically (lexical would say "570.124.06" < "570.26").
        // Driver 570.124.06 must clear 12.8's 570.26 minimum -> ceiling >= 12.8.
        let d = linux_driver("570.124.06", GpuClass::DataCenter);
        let ceiling = e.max_toolkit_for_driver(&d).unwrap();
        assert!(ceiling >= Version::parse("12.8").unwrap(), "got {ceiling:?}");
        // 12.9 min = 575.51.03 (>570.124.06) so ceiling stays 12.8.
        assert_eq!(ceiling, Version::parse("12.8").unwrap());
    }

    #[test]
    fn ceiling_windows_skips_na_13x_rows() {
        let e = DefaultCompatEngine::new();
        // A high Windows driver still cannot reach any 13.x (all N/A) -> caps at 12.9.
        let d = windows_driver("999.99");
        assert_eq!(e.max_toolkit_for_driver(&d).unwrap(), Version::parse("12.9").unwrap());
    }

    #[test]
    fn check_toolkit_strict_blocks_below_exact_minimum() {
        let e = DefaultCompatEngine::new();
        // 12.4 strict min = 550.54.14; driver 545.x is below -> Block.
        let d = linux_driver("545.23.06", GpuClass::GeForce);
        let out = e.check_toolkit(&d, &Version::parse("12.4").unwrap(), true);
        assert_eq!(out.severity, CompatSeverity::Block);
        assert!(!out.ok);
    }

    #[test]
    fn check_toolkit_strict_ok_at_or_above_minimum() {
        let e = DefaultCompatEngine::new();
        let d = linux_driver("550.54.14", GpuClass::GeForce);
        let out = e.check_toolkit(&d, &Version::parse("12.4").unwrap(), true);
        assert_eq!(out.severity, CompatSeverity::Ok);
        assert!(out.ok);
    }

    #[test]
    fn check_toolkit_likely_warns_above_floor_below_strict() {
        let e = DefaultCompatEngine::new();
        // Non-strict: 12.x floor = 525.60.13. Driver 540 >= floor but < 12.4 strict
        // (550.54.14) -> Warn (minor-version-compat likely-works), not Block.
        let d = linux_driver("540.00.00", GpuClass::GeForce);
        let out = e.check_toolkit(&d, &Version::parse("12.4").unwrap(), false);
        assert_eq!(out.severity, CompatSeverity::Warn);
        assert!(!out.ok);
    }

    #[test]
    fn check_toolkit_likely_blocks_below_floor() {
        let e = DefaultCompatEngine::new();
        // 13.x floor = 580.65.06; driver 560 is below the floor -> Block even non-strict.
        let d = linux_driver("560.28.03", GpuClass::DataCenter);
        let out = e.check_toolkit(&d, &Version::parse("13.0").unwrap(), false);
        assert_eq!(out.severity, CompatSeverity::Block);
    }

    #[test]
    fn check_toolkit_forward_compat_flag_only_for_eligible_gpu_on_linux() {
        let e = DefaultCompatEngine::new();
        // Below strict, DataCenter Linux -> forward_compat_possible = true.
        let dc = linux_driver("545.23.06", GpuClass::DataCenter);
        let out_dc = e.check_toolkit(&dc, &Version::parse("12.4").unwrap(), true);
        assert!(out_dc.forward_compat_possible);
        // GeForce never qualifies for cuda-compat.
        let gf = linux_driver("545.23.06", GpuClass::GeForce);
        let out_gf = e.check_toolkit(&gf, &Version::parse("12.4").unwrap(), true);
        assert!(!out_gf.forward_compat_possible);
        // Windows never qualifies (Linux only).
        let win = windows_driver("520.06");
        let out_win = e.check_toolkit(&win, &Version::parse("12.4").unwrap(), true);
        assert!(!out_win.forward_compat_possible);
    }

    #[test]
    fn pair_cudnn_picks_newest_supporting_line() {
        let e = DefaultCompatEngine::new();
        let avail = vec![
            Version::parse("8.9.7").unwrap(),
            Version::parse("9.23.0").unwrap(),
        ];
        // CUDA 13 -> only 9.x supports it.
        assert_eq!(
            e.pair_cudnn(&Version::parse("13.0").unwrap(), &avail),
            Some(Version::parse("9.23.0").unwrap())
        );
        // CUDA 11 -> only 8.x.
        assert_eq!(
            e.pair_cudnn(&Version::parse("11.8").unwrap(), &avail),
            Some(Version::parse("8.9.7").unwrap())
        );
        // CUDA 12 -> both support; pick newest (9.23.0).
        assert_eq!(
            e.pair_cudnn(&Version::parse("12.4").unwrap(), &avail),
            Some(Version::parse("9.23.0").unwrap())
        );
    }

    #[test]
    fn pair_cudnn_none_when_nothing_supports() {
        let e = DefaultCompatEngine::new();
        let avail = vec![Version::parse("8.9.7").unwrap()];
        // 8.9.7 supports [11,12]; CUDA 13 is unsupported -> None.
        assert_eq!(e.pair_cudnn(&Version::parse("13.0").unwrap(), &avail), None);
    }

    #[test]
    fn validate_pair_blocks_13x_with_8x_cudnn() {
        let e = DefaultCompatEngine::new();
        // CUDA 13.x requires cuDNN 9.x; pairing with 8.9.7 must Block.
        let out = e.validate_pair(
            &Version::parse("13.0").unwrap(),
            &Version::parse("8.9.7").unwrap(),
        );
        assert_eq!(out.severity, CompatSeverity::Block);
        assert!(!out.ok);
    }

    #[test]
    fn validate_pair_blocks_11x_with_9x_cudnn() {
        let e = DefaultCompatEngine::new();
        // CUDA 11.x requires cuDNN 8.x; 9.23.0 supports [12,13] only -> Block.
        let out = e.validate_pair(
            &Version::parse("11.8").unwrap(),
            &Version::parse("9.23.0").unwrap(),
        );
        assert_eq!(out.severity, CompatSeverity::Block);
    }

    #[test]
    fn validate_pair_ok_for_supported_major() {
        let e = DefaultCompatEngine::new();
        let out = e.validate_pair(
            &Version::parse("13.3").unwrap(),
            &Version::parse("9.23.0").unwrap(),
        );
        assert_eq!(out.severity, CompatSeverity::Ok);
        assert!(out.ok);
    }
}
```

- [ ] **Step 2 — Run it, see it fail.** `cargo test -p cuvm-core compat::engine::` — Expected: **fail** with `error[E0433]: failed to resolve: use of undeclared type 'DefaultCompatEngine'` (and `CompatSeverity`/`CompatOutcome` undeclared). Also add `pub mod engine;` to `crates/cuvm-core/src/compat/mod.rs`:
```rust
pub mod engine;
pub mod tables;

pub use engine::{CompatOutcome, CompatSeverity, DefaultCompatEngine};
```

- [ ] **Step 3 — Minimal implementation.** Prepend above the test module in `crates/cuvm-core/src/compat/engine.rs`:

```rust
//! Pure compatibility engine over the embedded §12 tables.
//!
//! Lives in `cuvm-core` to keep the algorithm I/O-free and unit-testable. The
//! `cuvm-app::CompatEngine` trait is implemented as a thin adapter in WU-8
//! (mapping [`CompatOutcome`] -> `app::Verdict`). All version comparisons use
//! `Version`'s numeric tuple `Ord` (spec §2.4: never lexical).

use crate::compat::tables::{CudnnMatrix, DriverCeilingTable};
use crate::platform::Os;
use crate::version::Version;
use crate::{Driver, GpuClass};

/// Core-side severity (mirrors `app::Severity`; kept separate so `cuvm-core`
/// owns no `cuvm-app` dependency — the Dependency Rule, spec §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatSeverity {
    Ok,
    Warn,
    Block,
}

/// Core-side verdict. WU-8 maps this onto `app::Verdict`.
#[derive(Debug, Clone)]
pub struct CompatOutcome {
    pub ok: bool,
    pub severity: CompatSeverity,
    pub reason: String,
    pub forward_compat_possible: bool,
}

/// Minor-version-compatibility floors ("likely works") from spec §12/§2.4.
const FLOOR_12X: &str = "525.60.13";
const FLOOR_13X: &str = "580.65.06";

/// Default `CompatEngine` over the embedded tables.
pub struct DefaultCompatEngine {
    drivers: DriverCeilingTable,
    cudnn: CudnnMatrix,
}

impl Default for DefaultCompatEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultCompatEngine {
    pub fn new() -> Self {
        DefaultCompatEngine {
            drivers: DriverCeilingTable::load(),
            cudnn: CudnnMatrix::load(),
        }
    }

    /// Per-OS minimum driver for a CUDA release, or `None` if that OS is N/A
    /// (e.g. all of CUDA 13.x on Windows).
    fn os_min<'a>(&'a self, row: &'a crate::compat::tables::DriverRow, os: Os) -> Option<&'a Version> {
        match os {
            Os::Linux => Some(&row.linux_min),
            Os::Windows => row.windows_min.as_ref(),
        }
    }

    /// The minor-version-compat floor for a toolkit's CUDA major.
    fn floor_for(&self, toolkit: &Version) -> Version {
        let s = if toolkit.major() >= 13 { FLOOR_13X } else { FLOOR_12X };
        Version::parse(s).expect("embedded floor constant parses")
    }

    /// `cuda-compat` is Linux-only and never applies to GeForce (spec §2.4).
    fn forward_compat_eligible(&self, d: &Driver) -> bool {
        d.platform.os == Os::Linux
            && matches!(
                d.gpu_class,
                GpuClass::DataCenter | GpuClass::NgcReadyRtx | GpuClass::Jetson
            )
    }

    /// Inverse lookup: highest CUDA whose per-OS minimum ≤ driver.
    pub fn max_toolkit_for_driver(&self, d: &Driver) -> Result<Version, CompatLookupError> {
        let os = d.platform.os;
        let mut best: Option<&Version> = None;
        for row in &self.drivers.rows {
            if let Some(min) = self.os_min(row, os) {
                if *min <= d.version {
                    match best {
                        Some(b) if row.cuda <= *b => {}
                        _ => best = Some(&row.cuda),
                    }
                }
            }
        }
        best.cloned().ok_or(CompatLookupError::NoCeiling)
    }

    /// Strict (exact per-release minimum) or likely (minor-version floor) check.
    pub fn check_toolkit(&self, d: &Driver, want: &Version, strict: bool) -> CompatOutcome {
        let fwd = self.forward_compat_eligible(d);
        let os = d.platform.os;

        let row = match self.drivers.row_for(want) {
            Some(r) => r,
            None => {
                return CompatOutcome {
                    ok: false,
                    severity: CompatSeverity::Block,
                    reason: format!("unknown CUDA toolkit {} (not in compat table)", want.raw),
                    forward_compat_possible: fwd,
                }
            }
        };

        let strict_min = match self.os_min(row, os) {
            Some(m) => m,
            None => {
                return CompatOutcome {
                    ok: false,
                    severity: CompatSeverity::Block,
                    reason: format!("CUDA {} is N/A on this OS", want.raw),
                    forward_compat_possible: fwd,
                }
            }
        };

        if d.version >= *strict_min {
            return CompatOutcome {
                ok: true,
                severity: CompatSeverity::Ok,
                reason: format!(
                    "driver {} satisfies CUDA {} minimum {}",
                    d.version.raw, want.raw, strict_min.raw
                ),
                forward_compat_possible: fwd,
            };
        }

        if strict {
            return CompatOutcome {
                ok: false,
                severity: CompatSeverity::Block,
                reason: format!(
                    "driver {} is below CUDA {} minimum {} (use --force or cuda-compat)",
                    d.version.raw, want.raw, strict_min.raw
                ),
                forward_compat_possible: fwd,
            };
        }

        // Non-strict: above the minor-version floor -> likely works (Warn).
        let floor = self.floor_for(want);
        if d.version >= floor {
            CompatOutcome {
                ok: false,
                severity: CompatSeverity::Warn,
                reason: format!(
                    "driver {} below strict minimum {} but above the {}.x minor-version floor {} (likely works)",
                    d.version.raw, strict_min.raw, want.major(), floor.raw
                ),
                forward_compat_possible: fwd,
            }
        } else {
            CompatOutcome {
                ok: false,
                severity: CompatSeverity::Block,
                reason: format!(
                    "driver {} below the {}.x minor-version floor {}",
                    d.version.raw, want.major(), floor.raw
                ),
                forward_compat_possible: fwd,
            }
        }
    }

    /// Newest available cuDNN whose line supports the toolkit's CUDA major.
    pub fn pair_cudnn(&self, toolkit: &Version, available: &[Version]) -> Option<Version> {
        let major = toolkit.major();
        available
            .iter()
            .filter(|cand| {
                self.cudnn
                    .entry_for(cand)
                    .map(|e| e.cuda_majors.contains(&major))
                    .unwrap_or(false)
            })
            .max()
            .cloned()
    }

    /// Validate an explicit toolkit/cuDNN pairing by CUDA major.
    pub fn validate_pair(&self, toolkit: &Version, cudnn: &Version) -> CompatOutcome {
        let major = toolkit.major();
        let supported = self
            .cudnn
            .entry_for(cudnn)
            .map(|e| e.cuda_majors.contains(&major))
            .unwrap_or(false);
        if supported {
            CompatOutcome {
                ok: true,
                severity: CompatSeverity::Ok,
                reason: format!("cuDNN {} supports CUDA {}.x", cudnn.raw, major),
                forward_compat_possible: false,
            }
        } else {
            let need = if major >= 13 { "9.x" } else if major <= 11 { "8.x" } else { "8.x/9.x" };
            CompatOutcome {
                ok: false,
                severity: CompatSeverity::Block,
                reason: format!(
                    "cuDNN {} does not support CUDA {}.x (needs cuDNN {})",
                    cudnn.raw, major, need
                ),
                forward_compat_possible: false,
            }
        }
    }
}

/// Error for `max_toolkit_for_driver` when no row matches.
#[derive(Debug, thiserror::Error)]
pub enum CompatLookupError {
    #[error("no CUDA toolkit ceiling for this driver/OS")]
    NoCeiling,
}
```

- [ ] **Step 4 — Run tests, see pass.** `cargo test -p cuvm-core compat::engine::` — Expected: `test result: ok. 15 passed; 0 failed`. Then run the whole crate to confirm no regressions: `cargo test -p cuvm-core` — Expected: all green (21 compat tests + earlier WU tests).

- [ ] **Step 5 — Commit.**
```bash
git add crates/cuvm-core/src/compat/engine.rs crates/cuvm-core/src/compat/mod.rs && git commit -m "feat(core): DefaultCompatEngine (ceiling inverse-lookup, strict/likely, cuDNN pairing)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 7.4 — `cuvm-nvidia` `SmiProbe` (nvidia-smi driver parse, graceful-absent)

**Files:**
- Create: `crates/cuvm-nvidia/src/smi.rs`
- Modify: `crates/cuvm-nvidia/src/lib.rs` (add `pub mod smi;` + re-export)
- Modify: `crates/cuvm-nvidia/Cargo.toml` (deps: `cuvm-core` path, `anyhow`, `thiserror`; dev: `tempfile`)
- Test: inline `#[cfg(test)]` in `crates/cuvm-nvidia/src/smi.rs`

`SmiProbe` implements the `DriverProbe` port (`fn probe(&self) -> Result<Driver>`). It shells out to `nvidia-smi --query-gpu=driver_version,name --format=csv,noheader`. The version-parsing logic is split into a pure free function `parse_smi_csv(&str) -> Result<(Version, GpuClass)>` so it is testable without a GPU. Missing `nvidia-smi` (spawn `ErrorKind::NotFound`, or a non-zero exit) must yield a **driver-unknown** `Driver { present: false, .. }` — NEVER a crash (spec §11: "Missing nvidia-smi → driver unknown, build-only OK").

The binary path is overridable via a constructor field so tests inject a fake `nvidia-smi` script (a tempfile) and assert the parse; the absent case is tested by pointing at a non-existent binary name.

Steps:

- [ ] **Step 1 — Write the failing tests.** Create `crates/cuvm-nvidia/src/smi.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_core::version::Version;
    use cuvm_core::GpuClass;

    #[test]
    fn parses_driver_version_and_geforce_class() {
        let csv = "550.54.14, NVIDIA GeForce RTX 4090\n";
        let (ver, class) = parse_smi_csv(csv).unwrap();
        assert_eq!(ver, Version::parse("550.54.14").unwrap());
        assert_eq!(class, GpuClass::GeForce);
    }

    #[test]
    fn parses_datacenter_class() {
        let csv = "535.54.03, NVIDIA A100-SXM4-80GB\n";
        let (_ver, class) = parse_smi_csv(csv).unwrap();
        assert_eq!(class, GpuClass::DataCenter);
    }

    #[test]
    fn parses_jetson_class() {
        let csv = "540.00.00, Orin (nvgpu)\n";
        let (_ver, class) = parse_smi_csv(csv).unwrap();
        assert_eq!(class, GpuClass::Jetson);
    }

    #[test]
    fn driver_version_parsed_as_numeric_tuple_not_lexical() {
        // 570.124.06 must compare > 570.26 numerically (the §2.4 trap).
        let (a, _) = parse_smi_csv("570.124.06, NVIDIA H100\n").unwrap();
        let (b, _) = parse_smi_csv("570.26, NVIDIA H100\n").unwrap();
        assert!(a > b);
    }

    #[test]
    fn empty_output_is_an_error_not_a_panic() {
        assert!(parse_smi_csv("\n").is_err());
        assert!(parse_smi_csv("").is_err());
    }

    #[test]
    fn probe_returns_driver_unknown_when_smi_missing() {
        // Point at a binary that does not exist -> graceful absent, never a crash.
        let probe = SmiProbe::with_binary("definitely-not-nvidia-smi-xyz");
        let d = probe.probe().expect("probe must not error when smi absent");
        assert!(!d.present, "absent nvidia-smi must yield present=false");
        assert_eq!(d.gpu_class, GpuClass::Unknown);
    }

    #[cfg(unix)]
    #[test]
    fn probe_parses_a_fake_nvidia_smi_script() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("nvidia-smi");
        let mut f = std::fs::File::create(&fake).unwrap();
        // Ignores args; prints one GPU row in the queried CSV shape.
        writeln!(f, "#!/bin/sh\necho '550.54.14, NVIDIA GeForce RTX 4090'").unwrap();
        let mut perms = std::fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake, perms).unwrap();

        let probe = SmiProbe::with_binary(fake.to_str().unwrap());
        let d = probe.probe().unwrap();
        assert!(d.present);
        assert_eq!(d.version, Version::parse("550.54.14").unwrap());
        assert_eq!(d.gpu_class, GpuClass::GeForce);
    }
}
```

- [ ] **Step 2 — Run it, see it fail.** `cargo test -p cuvm-nvidia smi::` — Expected: **fail** with `error[E0432]: unresolved import` / `cannot find function 'parse_smi_csv'` / `cannot find type 'SmiProbe'`. Also add to `crates/cuvm-nvidia/src/lib.rs`:
```rust
pub mod smi;

pub use smi::SmiProbe;
```
And confirm `crates/cuvm-nvidia/Cargo.toml` has (adding under `[dependencies]` / `[dev-dependencies]`):
```toml
[dependencies]
cuvm-core = { path = "../cuvm-core" }
anyhow = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 3 — Minimal implementation.** Prepend above the test module in `crates/cuvm-nvidia/src/smi.rs`:

```rust
//! `nvidia-smi` driver probe (spec §2.4/§11).
//!
//! Read-only. Shells out to `nvidia-smi`; on a missing binary or non-zero exit
//! it returns a *driver-unknown* [`Driver`] (`present: false`) rather than an
//! error — "missing nvidia-smi -> driver unknown, build-only OK" (spec §11).

use std::process::Command;

use cuvm_core::platform::{Arch, Os, Platform};
use cuvm_core::version::Version;
use cuvm_core::{Driver, GpuClass};

/// Probe implementing the `DriverProbe` port. `binary` is overridable for tests.
pub struct SmiProbe {
    binary: String,
}

impl Default for SmiProbe {
    fn default() -> Self {
        SmiProbe { binary: "nvidia-smi".to_string() }
    }
}

impl SmiProbe {
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the binary path/name (used by tests with a fake script).
    pub fn with_binary(binary: impl Into<String>) -> Self {
        SmiProbe { binary: binary.into() }
    }

    /// Probe the driver. Never errors on an absent `nvidia-smi`.
    pub fn probe(&self) -> anyhow::Result<Driver> {
        let plat = host_platform();
        let output = Command::new(&self.binary)
            .args([
                "--query-gpu=driver_version,name",
                "--format=csv,noheader",
            ])
            .output();

        let stdout = match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
            // Non-zero exit (e.g. "No devices were found") -> driver unknown.
            Ok(_) => return Ok(driver_unknown(plat)),
            // Spawn failed (NotFound / permission / etc.) -> driver unknown.
            Err(_) => return Ok(driver_unknown(plat)),
        };

        match parse_smi_csv(&stdout) {
            Ok((version, gpu_class)) => Ok(Driver {
                present: true,
                version,
                platform: plat,
                gpu_class,
            }),
            // Unparseable output is treated as unknown, not a hard failure.
            Err(_) => Ok(driver_unknown(plat)),
        }
    }
}

/// A "driver unknown" record: present=false, version 0, class Unknown.
fn driver_unknown(platform: Platform) -> Driver {
    Driver {
        present: false,
        version: Version::parse("0").expect("0 parses"),
        platform,
        gpu_class: GpuClass::Unknown,
    }
}

/// Host platform for the probe result. Arch detection beyond x86_64 is out of
/// scope here (spec gates arm64 behind its own integration run); default mirror.
fn host_platform() -> Platform {
    let os = if cfg!(windows) { Os::Windows } else { Os::Linux };
    let arch = if cfg!(target_arch = "aarch64") {
        Arch::Aarch64
    } else {
        Arch::X86_64
    };
    Platform { os, arch }
}

/// Pure parser for one `nvidia-smi` CSV row: `<driver_version>, <gpu name>`.
/// Returns the first GPU's driver version and inferred class.
pub fn parse_smi_csv(s: &str) -> anyhow::Result<(Version, GpuClass)> {
    let line = s
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .ok_or_else(|| anyhow::anyhow!("nvidia-smi produced no GPU rows"))?;

    let mut parts = line.splitn(2, ',');
    let ver_str = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing driver_version field"))?;
    let name = parts.next().map(str::trim).unwrap_or("");

    let version = Version::parse(ver_str)?;
    let gpu_class = classify_gpu(name);
    Ok((version, gpu_class))
}

/// Best-effort GPU-class inference from the marketing name. Conservative:
/// anything unrecognized is `Unknown` (so cuda-compat is never suggested for it).
fn classify_gpu(name: &str) -> GpuClass {
    let n = name.to_ascii_lowercase();
    if n.contains("geforce") || n.contains("titan") {
        GpuClass::GeForce
    } else if n.contains("jetson")
        || n.contains("orin")
        || n.contains("xavier")
        || n.contains("tegra")
    {
        GpuClass::Jetson
    } else if n.contains("a100")
        || n.contains("h100")
        || n.contains("h200")
        || n.contains("b200")
        || n.contains("a30")
        || n.contains("a40")
        || n.contains("l40")
        || n.contains("tesla")
        || n.contains("-sxm")
        || n.contains("nvidia a")
        || n.contains("nvidia h")
    {
        GpuClass::DataCenter
    } else {
        GpuClass::Unknown
    }
}
```

- [ ] **Step 4 — Run tests, see pass.** `cargo test -p cuvm-nvidia smi::` — Expected: `test result: ok. 7 passed; 0 failed` (6 cross-platform + 1 unix-only fake-script test on the Linux lane; the unix test is `#[cfg(unix)]`-gated and simply not counted on the windows CI lane). Then `cargo test -p cuvm-nvidia` — Expected: all green.

- [ ] **Step 5 — Sanity-build the whole workspace** to confirm the new modules compile on every target lane. `cargo build --workspace` — Expected: `Finished` with no errors. (On the windows CI lane the `#[cfg(unix)]` fake-script test is excluded; the absent-binary and pure-parser tests run on both lanes and prove the graceful-absent contract platform-independently.)

- [ ] **Step 6 — Commit.**
```bash
git add crates/cuvm-nvidia/src/smi.rs crates/cuvm-nvidia/src/lib.rs crates/cuvm-nvidia/Cargo.toml && git commit -m "feat(nvidia): nvidia-smi driver probe with graceful-absent fallback

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

**WU-7 done when:** `cargo test -p cuvm-core compat::` and `cargo test -p cuvm-nvidia smi::` are both green; the embedded tables encode spec §12 exactly (no 12.7; Windows N/A for all 13.x); the four regression assertions hold — (a) Linux 550.54.14 → ceiling 12.4, (b) Linux 565.x → 12.6, (c) tuple compare `570.26 < 570.124.06` drives the lookup (not lexical), (d) Windows N/A begins at CUDA 13.0; cuDNN rules (13.x↔9.x, 11.x↔8.x) enforced by `validate_pair`/`pair_cudnn`; and a missing `nvidia-smi` yields `Driver { present: false }` without crashing.

**Gate satisfied:** compat-data (corrected) — the Windows-13.0-N/A regression and numeric-tuple compare are encoded as tests, per spec §2.4 / §14 spike→unit gating.

**Wiring note for WU-8:** the `cuvm-app::CompatEngine` trait impl (mapping `core::CompatOutcome` → `app::Verdict`) and the `DriverProbe` impl forwarding `SmiProbe::probe` are added in the composition root in WU-8; `cuvm-app` already declares both traits (WU-1). No `cuvm-app` source changes are required in WU-7 — it is listed in cratesTouched only because WU-8 consumes these types through its ports.

---

### WU-8: M1 command wiring + doctor v1

**Gates:** WU-2 (Resolver + version grammar), WU-3 (Manifest + Inventory atomic I/O), WU-5 (Linux Activator + `CUVM_INJECTED` cleanup), WU-7 (Compat engine + embedded tables). All four MUST be merged to `main` before starting WU-8; this WU only *wires* their public surface and adds the `doctor` v1 use-case.

This WU has two halves:
1. **`doctor` v1** — a pure use-case in `cuvm-app` (`DoctorReport` + `run_doctor`) that takes the already-probed `Driver`, the active `Bundle` (if any), the live `PATH`/`LD_LIBRARY_PATH`/`CUDA_HOME` env strings, and the `CompatEngine`, and produces an ordered list of `Finding`s plus a machine-readable exit code. No I/O lives in `cuvm-app`; the CLI reads env and passes strings in.
2. **CLI wiring** in `cuvm-cli` — `ls`, `current`, `which`, `use`, `default`, `alias`, `unalias`, `pin`, `doctor` clap subcommands on top of the WU-1 composition root.

All backend dispatch for emission goes through `cuvm_platform::new_activator(os)` (runtime), per the contract. `default`'s opt-in symlink is the only syscall in this WU and is gated `#[cfg(unix)]` with a non-unix stub (Windows junction is WU-9).

---

#### Task 8.1 — `Severity`/`Finding`/`DoctorReport` types + exit-code mapping in cuvm-app

**Files:**
- Create: `crates/cuvm-app/src/doctor.rs`
- Modify: `crates/cuvm-app/src/lib.rs` (add `pub mod doctor;`)
- Test: inline `#[cfg(test)]` in `crates/cuvm-app/src/doctor.rs`

Note: `Severity { Ok, Warn, Block }` already exists in `cuvm-app` from WU-7 (it is part of `Verdict`). Reuse it; do **not** redefine.

1. - [ ] **Write the failing test** for the report types + exit-code contract. The contract: exit `0` if all findings are `Ok`, `1` if the worst finding is `Warn`, `2` if any finding is `Block`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_app::Severity; // re-exported from WU-7

    fn finding(sev: Severity) -> Finding {
        Finding { code: "X".into(), severity: sev, title: "t".into(), detail: "d".into(), hint: None }
    }

    #[test]
    fn exit_code_is_zero_when_all_ok() {
        let r = DoctorReport { findings: vec![finding(Severity::Ok), finding(Severity::Ok)] };
        assert_eq!(r.exit_code(), 0);
        assert!(r.is_healthy());
    }

    #[test]
    fn exit_code_is_one_when_worst_is_warn() {
        let r = DoctorReport { findings: vec![finding(Severity::Ok), finding(Severity::Warn)] };
        assert_eq!(r.exit_code(), 1);
        assert!(!r.is_healthy());
    }

    #[test]
    fn exit_code_is_two_when_any_block_even_with_warns() {
        let r = DoctorReport {
            findings: vec![finding(Severity::Warn), finding(Severity::Block), finding(Severity::Ok)],
        };
        assert_eq!(r.exit_code(), 2);
    }

    #[test]
    fn empty_report_is_healthy() {
        let r = DoctorReport { findings: vec![] };
        assert_eq!(r.exit_code(), 0);
        assert!(r.is_healthy());
    }
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-app doctor::tests`
     Expected: fail with `error[E0433]: ... use of undeclared ... DoctorReport` / `Finding` (the module is referenced before it exists).

3. - [ ] **Minimal implementation.** Add `pub mod doctor;` to `crates/cuvm-app/src/lib.rs`, then create the types in `crates/cuvm-app/src/doctor.rs`:

```rust
//! `doctor` v1 use-case: pure diagnostics over an already-probed environment.
//! No I/O here — the CLI reads env strings and the driver/bundle, and passes them in.

use crate::Severity;

/// One diagnostic line. `code` is a stable machine-readable id (e.g. "DRIVER_CEILING").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub hint: Option<String>,
}

/// The full ordered set of findings produced by one `doctor` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    pub findings: Vec<Finding>,
}

impl DoctorReport {
    /// Machine-readable exit code: 0 = all Ok, 1 = at least one Warn (no Block),
    /// 2 = at least one Block. Gates CI.
    pub fn exit_code(&self) -> i32 {
        let mut worst = 0;
        for f in &self.findings {
            let level = match f.severity {
                Severity::Ok => 0,
                Severity::Warn => 1,
                Severity::Block => 2,
            };
            if level > worst {
                worst = level;
            }
        }
        worst
    }

    pub fn is_healthy(&self) -> bool {
        self.exit_code() == 0
    }
}
```

4. - [ ] **Run tests, see pass.** Command: `cargo test -p cuvm-app doctor::tests`
     Expected: `test result: ok. 4 passed; 0 failed`.

5. - [ ] **Commit.**

```bash
git add crates/cuvm-app/src/doctor.rs crates/cuvm-app/src/lib.rs && git commit -m "feat(app): add DoctorReport/Finding types with machine-readable exit codes" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.2 — Driver↔toolkit ceiling check (`doctor` input: Driver + active Version)

**Files:**
- Modify: `crates/cuvm-app/src/doctor.rs`
- Test: inline `#[cfg(test)]` in `crates/cuvm-app/src/doctor.rs`

This produces the first real `Finding`. It uses the WU-7 `CompatEngine::check_toolkit` and `max_toolkit_for_driver`. We mock `CompatEngine` with `mockall`.

1. - [ ] **Write the failing test.** Three cases: active toolkit within ceiling (Ok), active toolkit exceeds ceiling (Warn + cuda-compat hint follows the engine's `forward_compat_possible`), and no driver present (Warn "driver unknown, build-only OK", never Block — per §11).

```rust
#[cfg(test)]
mod ceiling_tests {
    use super::*;
    use crate::{CompatEngine, Severity, Verdict};
    use cuvm_core::{Arch, Driver, GpuClass, Os, Platform, Version};
    use mockall::mock;

    mock! {
        pub Compat {}
        impl CompatEngine for Compat {
            fn max_toolkit_for_driver(&self, d: &Driver) -> anyhow::Result<Version>;
            fn check_toolkit(&self, d: &Driver, want: &Version, strict: bool) -> Verdict;
            fn pair_cudnn(&self, toolkit: &Version, available: &[Version]) -> Option<Version>;
            fn validate_pair(&self, toolkit: &Version, cudnn: &Version) -> Verdict;
        }
    }

    fn linux_driver(v: &str, present: bool, gpu: GpuClass) -> Driver {
        Driver {
            present,
            version: Version::parse(v).unwrap(),
            platform: Platform { os: Os::Linux, arch: Arch::X86_64 },
            gpu_class: gpu,
        }
    }

    #[test]
    fn within_ceiling_is_ok() {
        let mut engine = MockCompat::new();
        engine.expect_max_toolkit_for_driver()
            .returning(|_| Ok(Version::parse("12.6").unwrap()));
        engine.expect_check_toolkit().returning(|_, _, _| Verdict {
            ok: true, severity: Severity::Ok, reason: "within ceiling".into(),
            forward_compat_possible: false,
        });
        let d = linux_driver("565.57.01", true, GpuClass::GeForce);
        let active = Version::parse("12.4.1").unwrap();
        let f = check_driver_ceiling(&engine, &d, Some(&active));
        assert_eq!(f.code, "DRIVER_CEILING");
        assert_eq!(f.severity, Severity::Ok);
    }

    #[test]
    fn exceeds_ceiling_warns_with_compat_hint_on_eligible_gpu() {
        let mut engine = MockCompat::new();
        engine.expect_max_toolkit_for_driver()
            .returning(|_| Ok(Version::parse("12.4").unwrap()));
        engine.expect_check_toolkit().returning(|_, _, _| Verdict {
            ok: false, severity: Severity::Warn, reason: "toolkit exceeds driver ceiling".into(),
            forward_compat_possible: true,
        });
        let d = linux_driver("550.54.14", true, GpuClass::DataCenter);
        let active = Version::parse("12.9.0").unwrap();
        let f = check_driver_ceiling(&engine, &d, Some(&active));
        assert_eq!(f.severity, Severity::Warn);
        assert!(f.detail.contains("12.9.0"));
        assert!(f.detail.contains("12.4"));
        assert!(f.hint.as_deref().unwrap().contains("cuda-compat"));
    }

    #[test]
    fn no_driver_warns_build_only_ok_never_blocks() {
        let engine = MockCompat::new(); // engine never consulted when driver absent
        let d = linux_driver("0", false, GpuClass::Unknown);
        let active = Version::parse("12.4.1").unwrap();
        let f = check_driver_ceiling(&engine, &d, Some(&active));
        assert_eq!(f.code, "DRIVER_ABSENT");
        assert_eq!(f.severity, Severity::Warn);
        assert!(f.detail.to_lowercase().contains("build-only"));
    }

    #[test]
    fn no_active_toolkit_reports_driver_ceiling_only() {
        let mut engine = MockCompat::new();
        engine.expect_max_toolkit_for_driver()
            .returning(|_| Ok(Version::parse("12.6").unwrap()));
        let d = linux_driver("565.57.01", true, GpuClass::GeForce);
        let f = check_driver_ceiling(&engine, &d, None);
        assert_eq!(f.code, "DRIVER_CEILING");
        assert_eq!(f.severity, Severity::Ok);
        assert!(f.detail.contains("12.6"));
    }
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-app doctor::ceiling_tests`
     Expected: fail with `error[E0425]: cannot find function 'check_driver_ceiling' in this scope`.

3. - [ ] **Minimal implementation.** Append to `crates/cuvm-app/src/doctor.rs`:

```rust
use crate::CompatEngine;
use cuvm_core::{Driver, GpuClass, Version};

/// Driver→toolkit ceiling diagnostic. Never blocks (§11): an exceeded ceiling is a
/// Warn (with a --force/cuda-compat path), and a missing driver is "build-only OK".
pub fn check_driver_ceiling(
    engine: &dyn CompatEngine,
    driver: &Driver,
    active: Option<&Version>,
) -> Finding {
    if !driver.present {
        return Finding {
            code: "DRIVER_ABSENT".into(),
            severity: Severity::Warn,
            title: "GPU driver not detected".into(),
            detail: "nvidia-smi reported no driver; driver unknown, build-only OK.".into(),
            hint: Some("Install an NVIDIA driver to run CUDA programs; compilation still works.".into()),
        };
    }

    let ceiling = match engine.max_toolkit_for_driver(driver) {
        Ok(v) => v,
        Err(e) => {
            return Finding {
                code: "DRIVER_CEILING".into(),
                severity: Severity::Warn,
                title: "Could not determine driver ceiling".into(),
                detail: format!("driver {} ({}): {e}", driver.version.raw, "ceiling lookup failed"),
                hint: None,
            };
        }
    };

    let Some(active) = active else {
        return Finding {
            code: "DRIVER_CEILING".into(),
            severity: Severity::Ok,
            title: "Driver toolkit ceiling".into(),
            detail: format!(
                "driver {} supports CUDA up to {}; no toolkit is active.",
                driver.version.raw, ceiling.raw
            ),
            hint: None,
        };
    };

    let verdict = engine.check_toolkit(driver, active, false);
    if verdict.ok {
        return Finding {
            code: "DRIVER_CEILING".into(),
            severity: Severity::Ok,
            title: "Active toolkit within driver ceiling".into(),
            detail: format!(
                "active CUDA {} <= driver ceiling {} (driver {}).",
                active.raw, ceiling.raw, driver.version.raw
            ),
            hint: None,
        };
    }

    let eligible = matches!(
        driver.gpu_class,
        GpuClass::DataCenter | GpuClass::Jetson | GpuClass::NgcReadyRtx
    );
    let hint = if verdict.forward_compat_possible && eligible {
        Some(
            "This GPU class is cuda-compat eligible (Linux): a forward-compat package may raise the ceiling."
                .into(),
        )
    } else {
        Some("Re-run with --force to proceed, or switch to a toolkit within the ceiling.".into())
    };

    Finding {
        code: "DRIVER_CEILING".into(),
        severity: verdict.severity,
        title: "Active toolkit exceeds driver ceiling".into(),
        detail: format!(
            "active CUDA {} exceeds driver ceiling {} (driver {}).",
            active.raw, ceiling.raw, driver.version.raw
        ),
        hint,
    }
}
```

4. - [ ] **Run tests, see pass.** Command: `cargo test -p cuvm-app doctor::ceiling_tests`
     Expected: `test result: ok. 4 passed; 0 failed`.

5. - [ ] **Commit.**

```bash
git add crates/cuvm-app/src/doctor.rs && git commit -m "feat(app): doctor driver-to-toolkit ceiling check (warn-not-block, cuda-compat hint)" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.3 — PATH / LD_LIBRARY_PATH hygiene: dup CUDA dirs + stale entries + nvcc-vs-CUDA_HOME mismatch

**Files:**
- Modify: `crates/cuvm-app/src/doctor.rs`
- Test: inline `#[cfg(test)]` in `crates/cuvm-app/src/doctor.rs`

This is the core of doctor v1. Input is pre-read strings so it stays pure: `EnvSnapshot { path, ld_library_path, cuda_home, active_root }`. A "CUDA entry" is a path segment whose components contain a `cuda` token (case-insensitive) and that ends in `/bin` (for PATH) or `/lib64`|`/lib` (for LD). Per §11 we detect: (a) duplicate CUDA dirs, (b) a CUDA `bin` on PATH that does not belong to the active root (stale), and (c) the first `nvcc` resolvable on PATH not living under `CUDA_HOME`.

1. - [ ] **Write the failing test.** The deliberately broken PATH from the WU brief: two different CUDA `bin` dirs (dup-class), the first being a stale `12.2` install, with `CUDA_HOME` pointing at `12.4` — so nvcc resolves to 12.2 (mismatch). Expected: a `PATH_DUP_CUDA` Warn and an `NVCC_MISMATCH` Block, plus a clean case yielding `Ok`.

```rust
#[cfg(test)]
mod hygiene_tests {
    use super::*;
    use crate::Severity;

    fn snap(path: &str, ld: &str, home: &str, root: &str) -> EnvSnapshot {
        EnvSnapshot {
            path: path.into(),
            ld_library_path: ld.into(),
            cuda_home: Some(home.into()),
            active_root: Some(root.into()),
            path_sep: ':',
        }
    }

    #[test]
    fn clean_single_cuda_on_path_is_ok() {
        let s = snap(
            "/home/u/.cuvm/versions/12.4.1/bin:/usr/bin",
            "/home/u/.cuvm/versions/12.4.1/lib64",
            "/home/u/.cuvm/versions/12.4.1",
            "/home/u/.cuvm/versions/12.4.1",
        );
        let fs = check_path_hygiene(&s);
        assert!(fs.iter().all(|f| f.severity == Severity::Ok), "{fs:#?}");
    }

    #[test]
    fn dup_cuda_dirs_warn() {
        let s = snap(
            "/opt/cuda-12.2/bin:/home/u/.cuvm/versions/12.4.1/bin:/usr/bin",
            "/opt/cuda-12.2/lib64:/home/u/.cuvm/versions/12.4.1/lib64",
            "/home/u/.cuvm/versions/12.4.1",
            "/home/u/.cuvm/versions/12.4.1",
        );
        let fs = check_path_hygiene(&s);
        let dup = fs.iter().find(|f| f.code == "PATH_DUP_CUDA").expect("dup finding");
        assert_eq!(dup.severity, Severity::Warn);
        assert!(dup.detail.contains("/opt/cuda-12.2/bin"));
        assert!(dup.detail.contains("12.4.1/bin"));
    }

    #[test]
    fn nvcc_resolving_outside_cuda_home_blocks() {
        // stale 12.2 bin comes FIRST on PATH, so nvcc resolves there, but CUDA_HOME=12.4.1
        let s = snap(
            "/opt/cuda-12.2/bin:/home/u/.cuvm/versions/12.4.1/bin:/usr/bin",
            "/opt/cuda-12.2/lib64:/home/u/.cuvm/versions/12.4.1/lib64",
            "/home/u/.cuvm/versions/12.4.1",
            "/home/u/.cuvm/versions/12.4.1",
        );
        let fs = check_path_hygiene(&s);
        let mm = fs.iter().find(|f| f.code == "NVCC_MISMATCH").expect("mismatch finding");
        assert_eq!(mm.severity, Severity::Block);
        assert!(mm.detail.contains("/opt/cuda-12.2/bin"));
        assert!(mm.detail.contains("/home/u/.cuvm/versions/12.4.1"));
        assert!(mm.hint.as_deref().unwrap().contains("cuvm use"));
    }

    #[test]
    fn stale_cuda_bin_not_matching_active_root_warns() {
        let s = snap(
            "/opt/cuda-11.8/bin:/usr/bin",
            "/opt/cuda-11.8/lib64",
            "/home/u/.cuvm/versions/12.4.1",
            "/home/u/.cuvm/versions/12.4.1",
        );
        let fs = check_path_hygiene(&s);
        let stale = fs.iter().find(|f| f.code == "PATH_STALE_CUDA").expect("stale finding");
        assert_eq!(stale.severity, Severity::Warn);
        assert!(stale.detail.contains("/opt/cuda-11.8/bin"));
    }

    #[test]
    fn no_cuda_home_yields_no_mismatch_only_info() {
        let s = EnvSnapshot {
            path: "/usr/bin".into(),
            ld_library_path: String::new(),
            cuda_home: None,
            active_root: None,
            path_sep: ':',
        };
        let fs = check_path_hygiene(&s);
        assert!(fs.iter().all(|f| f.severity == Severity::Ok), "{fs:#?}");
    }
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-app doctor::hygiene_tests`
     Expected: fail with `cannot find type 'EnvSnapshot'` / `cannot find function 'check_path_hygiene'`.

3. - [ ] **Minimal implementation.** Append to `crates/cuvm-app/src/doctor.rs`:

```rust
/// Pre-read environment, passed in by the CLI so this module stays I/O-free.
/// `path_sep` is ':' on unix and ';' on windows (the CLI fills it from the OS).
#[derive(Debug, Clone)]
pub struct EnvSnapshot {
    pub path: String,
    pub ld_library_path: String,
    pub cuda_home: Option<String>,
    pub active_root: Option<String>,
    pub path_sep: char,
}

fn norm(p: &str) -> String {
    p.trim_end_matches(['/', '\\']).replace('\\', "/")
}

/// A segment is a "CUDA bin" if it ends in /bin and any component holds a "cuda" token.
fn is_cuda_bin(seg: &str) -> bool {
    let n = norm(seg).to_lowercase();
    n.ends_with("/bin") && n.contains("cuda")
}

fn is_cuda_lib(seg: &str) -> bool {
    let n = norm(seg).to_lowercase();
    (n.ends_with("/lib64") || n.ends_with("/lib")) && n.contains("cuda")
}

/// PATH/LD_LIBRARY_PATH hygiene: dup CUDA dirs, stale CUDA entries, nvcc vs CUDA_HOME.
pub fn check_path_hygiene(s: &EnvSnapshot) -> Vec<Finding> {
    let mut out = Vec::new();
    let active = s.active_root.as_deref().map(norm);

    let bins: Vec<&str> = s
        .path
        .split(s.path_sep)
        .filter(|seg| !seg.is_empty() && is_cuda_bin(seg))
        .collect();

    // (a) duplicate CUDA bin dirs on PATH.
    if bins.len() > 1 {
        out.push(Finding {
            code: "PATH_DUP_CUDA".into(),
            severity: Severity::Warn,
            title: "Multiple CUDA bin directories on PATH".into(),
            detail: format!(
                "PATH contains {} CUDA bin entries: {}",
                bins.len(),
                bins.join(", ")
            ),
            hint: Some("Run `cuvm use <ver>` to rebuild a clean CUVM_INJECTED segment.".into()),
        });
    }

    // (b) stale CUDA bins that do not belong to the active root.
    if let Some(active) = &active {
        let stale: Vec<&&str> = bins
            .iter()
            .filter(|b| !norm(b).starts_with(active.as_str()))
            .collect();
        if !stale.is_empty() {
            out.push(Finding {
                code: "PATH_STALE_CUDA".into(),
                severity: Severity::Warn,
                title: "Stale CUDA bin on PATH".into(),
                detail: format!(
                    "PATH has CUDA bin(s) outside the active toolkit {active}: {}",
                    stale.iter().map(|x| **x).collect::<Vec<_>>().join(", ")
                ),
                hint: Some("`cuvm use <ver>` strips CUVM_INJECTED precisely; remove manual PATH edits.".into()),
            });
        }
    }

    // (c) nvcc resolution vs CUDA_HOME: the FIRST CUDA bin on PATH is where nvcc resolves.
    if let (Some(home), Some(first_bin)) = (s.cuda_home.as_deref().map(norm), bins.first()) {
        let resolved_root = norm(first_bin)
            .strip_suffix("/bin")
            .map(str::to_string)
            .unwrap_or_default();
        if !resolved_root.is_empty() && resolved_root != home {
            out.push(Finding {
                code: "NVCC_MISMATCH".into(),
                severity: Severity::Block,
                title: "nvcc does not match CUDA_HOME".into(),
                detail: format!(
                    "nvcc resolves to {first_bin} (root {resolved_root}) but CUDA_HOME={home}; \
                     builds will use a different toolkit than the active one."
                ),
                hint: Some("Run `cuvm use <ver>` so the active bin is first on PATH.".into()),
            });
        }
    }

    // LD hygiene: dup CUDA lib dirs (mirrors PATH dup; warn only).
    let libs: Vec<&str> = s
        .ld_library_path
        .split(s.path_sep)
        .filter(|seg| !seg.is_empty() && is_cuda_lib(seg))
        .collect();
    if libs.len() > 1 {
        out.push(Finding {
            code: "LD_DUP_CUDA".into(),
            severity: Severity::Warn,
            title: "Multiple CUDA lib directories on LD_LIBRARY_PATH".into(),
            detail: format!("LD_LIBRARY_PATH has {} CUDA lib entries: {}", libs.len(), libs.join(", ")),
            hint: Some("`cuvm use <ver>` strips the recorded CUVM_INJECTED lib segment first.".into()),
        });
    }

    if out.is_empty() {
        out.push(Finding {
            code: "PATH_HYGIENE".into(),
            severity: Severity::Ok,
            title: "PATH / LD_LIBRARY_PATH clean".into(),
            detail: "no duplicate or stale CUDA entries; nvcc matches CUDA_HOME.".into(),
            hint: None,
        });
    }
    out
}
```

4. - [ ] **Run tests, see pass.** Command: `cargo test -p cuvm-app doctor::hygiene_tests`
     Expected: `test result: ok. 5 passed; 0 failed`.

5. - [ ] **Commit.**

```bash
git add crates/cuvm-app/src/doctor.rs && git commit -m "feat(app): doctor PATH/LD hygiene (dup, stale, nvcc-vs-CUDA_HOME mismatch)" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.4 — `run_doctor` aggregator + insta snapshot on the broken PATH

**Files:**
- Modify: `crates/cuvm-app/src/doctor.rs`
- Test: inline `#[cfg(test)]` in `crates/cuvm-app/src/doctor.rs`
- Test (golden): `crates/cuvm-app/src/doctor_snapshots/__snapshots__/doctor__broken_path_dup_and_nvcc_mismatch.snap` (insta-generated; checked in)

`run_doctor` composes the ceiling check (8.2) + hygiene checks (8.3) into one ordered `DoctorReport`. It takes the `Driver`, active `Version`, `EnvSnapshot`, and a `&dyn CompatEngine`. The snapshot test renders the report deterministically (a `Display` impl) so a broken PATH produces an exact diagnostic + the report's `exit_code()` is 2 (because of the `NVCC_MISMATCH` Block).

1. - [ ] **Write the failing test.** Includes a deterministic `Display` rendering and an `insta` snapshot of the broken-PATH report, plus an exit-code assertion. `mockall` provides the engine.

```rust
#[cfg(test)]
mod aggregate_tests {
    use super::*;
    use crate::{CompatEngine, Severity, Verdict};
    use cuvm_core::{Arch, Driver, GpuClass, Os, Platform, Version};
    use mockall::mock;

    mock! {
        pub Eng {}
        impl CompatEngine for Eng {
            fn max_toolkit_for_driver(&self, d: &Driver) -> anyhow::Result<Version>;
            fn check_toolkit(&self, d: &Driver, want: &Version, strict: bool) -> Verdict;
            fn pair_cudnn(&self, toolkit: &Version, available: &[Version]) -> Option<Version>;
            fn validate_pair(&self, toolkit: &Version, cudnn: &Version) -> Verdict;
        }
    }

    fn broken_snapshot() -> EnvSnapshot {
        EnvSnapshot {
            path: "/opt/cuda-12.2/bin:/home/u/.cuvm/versions/12.4.1/bin:/usr/bin".into(),
            ld_library_path: "/opt/cuda-12.2/lib64:/home/u/.cuvm/versions/12.4.1/lib64".into(),
            cuda_home: Some("/home/u/.cuvm/versions/12.4.1".into()),
            active_root: Some("/home/u/.cuvm/versions/12.4.1".into()),
            path_sep: ':',
        }
    }

    #[test]
    fn broken_path_dup_and_nvcc_mismatch_snapshot() {
        let mut engine = MockEng::new();
        engine.expect_max_toolkit_for_driver()
            .returning(|_| Ok(Version::parse("12.6").unwrap()));
        engine.expect_check_toolkit().returning(|_, _, _| Verdict {
            ok: true, severity: Severity::Ok, reason: "ok".into(), forward_compat_possible: false,
        });
        let driver = Driver {
            present: true,
            version: Version::parse("565.57.01").unwrap(),
            platform: Platform { os: Os::Linux, arch: Arch::X86_64 },
            gpu_class: GpuClass::GeForce,
        };
        let active = Version::parse("12.4.1").unwrap();
        let report = run_doctor(&engine, &driver, Some(&active), &broken_snapshot());

        // The NVCC_MISMATCH block drives a nonzero exit.
        assert_eq!(report.exit_code(), 2);
        insta::assert_snapshot!(report.to_string());
    }
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-app doctor::aggregate_tests`
     Expected: fail with `cannot find function 'run_doctor'` and `DoctorReport doesn't implement std::fmt::Display`.

3. - [ ] **Minimal implementation.** Append to `crates/cuvm-app/src/doctor.rs` (`run_doctor` + `Display`):

```rust
use std::fmt;

/// Compose every v1 diagnostic into one ordered report. Pure: all inputs are pre-read.
pub fn run_doctor(
    engine: &dyn CompatEngine,
    driver: &Driver,
    active: Option<&Version>,
    env: &EnvSnapshot,
) -> DoctorReport {
    let mut findings = Vec::new();
    findings.push(check_driver_ceiling(engine, driver, active));
    findings.extend(check_path_hygiene(env));
    DoctorReport { findings }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Severity::Ok => "OK",
            Severity::Warn => "WARN",
            Severity::Block => "BLOCK",
        };
        f.write_str(s)
    }
}

impl fmt::Display for DoctorReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for finding in &self.findings {
            writeln!(f, "[{}] {} ({})", finding.severity, finding.title, finding.code)?;
            writeln!(f, "    {}", finding.detail)?;
            if let Some(hint) = &finding.hint {
                writeln!(f, "    hint: {hint}")?;
            }
        }
        write!(f, "exit: {}", self.exit_code())
    }
}
```

4. - [ ] **Run tests; accept the snapshot.** Command: `cargo test -p cuvm-app doctor::aggregate_tests`
     Expected: first run reports a new pending snapshot (`1 snapshot to review`). Accept it:

```bash
cargo insta accept --package cuvm-app
```

   Then re-run `cargo test -p cuvm-app doctor::aggregate_tests` → Expected: `test result: ok. 1 passed`. The accepted `.snap` content asserts the exact diagnostic, e.g.:

```
---
source: crates/cuvm-app/src/doctor.rs
expression: report.to_string()
---
[OK] Active toolkit within driver ceiling (DRIVER_CEILING)
    active CUDA 12.4.1 <= driver ceiling 12.6 (driver 565.57.01).
[WARN] Multiple CUDA bin directories on PATH (PATH_DUP_CUDA)
    PATH contains 2 CUDA bin entries: /opt/cuda-12.2/bin, /home/u/.cuvm/versions/12.4.1/bin
    hint: Run `cuvm use <ver>` to rebuild a clean CUVM_INJECTED segment.
[WARN] Stale CUDA bin on PATH (PATH_STALE_CUDA)
    PATH has CUDA bin(s) outside the active toolkit /home/u/.cuvm/versions/12.4.1: /opt/cuda-12.2/bin
    hint: `cuvm use <ver>` strips CUVM_INJECTED precisely; remove manual PATH edits.
[BLOCK] nvcc does not match CUDA_HOME (NVCC_MISMATCH)
    nvcc resolves to /opt/cuda-12.2/bin (root /opt/cuda-12.2) but CUDA_HOME=/home/u/.cuvm/versions/12.4.1; builds will use a different toolkit than the active one.
    hint: Run `cuvm use <ver>` so the active bin is first on PATH.
[WARN] Multiple CUDA lib directories on LD_LIBRARY_PATH (LD_DUP_CUDA)
    LD_LIBRARY_PATH has 2 CUDA lib entries: /opt/cuda-12.2/lib64, /home/u/.cuvm/versions/12.4.1/lib64
    hint: `cuvm use <ver>` strips the recorded CUVM_INJECTED lib segment first.
exit: 2
```

5. - [ ] **Commit.**

```bash
git add crates/cuvm-app/src/doctor.rs crates/cuvm-app/src/snapshots crates/cuvm-app/src/doctor.rs.snap 2>/dev/null; git add -A crates/cuvm-app && git commit -m "feat(app): run_doctor aggregator + golden snapshot on a deliberately broken PATH" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.5 — CLI clap command tree for the M1 verbs + composition wiring

**Files:**
- Modify: `crates/cuvm-cli/src/cli.rs` (clap `Command` enum)
- Create: `crates/cuvm-cli/src/commands/mod.rs`
- Modify: `crates/cuvm-cli/src/main.rs` (dispatch)
- Modify: `crates/cuvm-cli/src/composition.rs` (build the WU-1 wired deps incl. `CompatEngine`, `DriverProbe`)
- Test: `crates/cuvm-cli/tests/m1_e2e.rs` (assert_cmd)

WU-0/1 already provide `Cli`, the `Command` enum stub, and the composition root. We add the M1 variants. The composition root already builds `Inventory`/`Resolver`/`Activator` (WU-1 factory); we add `CompatEngine` (WU-7) and `DriverProbe` (WU-1 stub probe is fine for M1).

1. - [ ] **Write the failing test** — `--help` lists all M1 subcommands (cheap surface lock, no env mutation).

```rust
use assert_cmd::Command;
use predicates::str::contains;

fn cuvm() -> Command {
    Command::cargo_bin("cuvm").expect("binary builds")
}

#[test]
fn help_lists_m1_commands() {
    cuvm()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("ls"))
        .stdout(contains("current"))
        .stdout(contains("which"))
        .stdout(contains("use"))
        .stdout(contains("default"))
        .stdout(contains("alias"))
        .stdout(contains("unalias"))
        .stdout(contains("pin"))
        .stdout(contains("doctor"));
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-cli --test m1_e2e help_lists_m1_commands`
     Expected: fail — `--help` does not yet contain `which`/`pin`/`doctor` (assertion failure on the missing substring).

3. - [ ] **Minimal implementation.** Add the variants to `crates/cuvm-cli/src/cli.rs`:

```rust
use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "cuvm", about = "CUDA toolkit version manager", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List installed bundles.
    Ls,
    /// Print the currently active bundle handle.
    Current,
    /// Print the absolute toolkit root for a spec.
    Which(WhichArgs),
    /// Print env-activation code for a spec (shim eval's it).
    Use(UseArgs),
    /// Set the persistent default (writes the `default` alias; --link adds the current pointer).
    Default(DefaultArgs),
    /// Create or update an alias.
    Alias(AliasArgs),
    /// Remove an alias.
    Unalias(UnaliasArgs),
    /// Write `.cuda-version` in the current directory.
    Pin(PinArgs),
    /// Diagnose driver/toolkit/PATH health; exit code is machine-readable.
    Doctor,
}

#[derive(Args, Debug)]
pub struct WhichArgs {
    pub spec: String,
}

#[derive(Args, Debug)]
pub struct UseArgs {
    /// Optional spec; omitted => resolve from .cuda-version / default.
    pub spec: Option<String>,
    /// Target shell for the emitted script.
    #[arg(long, value_enum, default_value_t = ShellArg::Bash)]
    pub shell: ShellArg,
}

#[derive(Args, Debug)]
pub struct DefaultArgs {
    pub spec: String,
    /// Also create the opt-in `current` symlink/junction pointer (§6).
    #[arg(long)]
    pub link: bool,
}

#[derive(Args, Debug)]
pub struct AliasArgs {
    pub name: String,
    pub target: String,
}

#[derive(Args, Debug)]
pub struct UnaliasArgs {
    pub name: String,
}

#[derive(Args, Debug)]
pub struct PinArgs {
    pub spec: String,
}

/// CLI-facing mirror of `cuvm_core::Shell` (clap ValueEnum).
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum ShellArg {
    Bash,
    Zsh,
    Powershell,
    Cmd,
}

impl From<ShellArg> for cuvm_core::Shell {
    fn from(s: ShellArg) -> Self {
        match s {
            ShellArg::Bash => cuvm_core::Shell::Bash,
            ShellArg::Zsh => cuvm_core::Shell::Zsh,
            ShellArg::Powershell => cuvm_core::Shell::PowerShell,
            ShellArg::Cmd => cuvm_core::Shell::Cmd,
        }
    }
}
```

   Create `crates/cuvm-cli/src/commands/mod.rs` (sub-handlers filled in later tasks; stubs now so it compiles):

```rust
pub mod alias;
pub mod current;
pub mod default;
pub mod doctor;
pub mod ls;
pub mod pin;
pub mod r#use;
pub mod which;
```

   Wire dispatch in `crates/cuvm-cli/src/main.rs`:

```rust
mod cli;
mod commands;
mod composition;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let exit = real_main();
    std::process::exit(exit);
}

fn real_main() -> i32 {
    let args = Cli::parse();
    let deps = composition::build();
    let result: anyhow::Result<i32> = match args.command {
        Command::Ls => commands::ls::run(&deps).map(|_| 0),
        Command::Current => commands::current::run(&deps).map(|_| 0),
        Command::Which(a) => commands::which::run(&deps, &a.spec).map(|_| 0),
        Command::Use(a) => commands::r#use::run(&deps, a.spec.as_deref(), a.shell.into()).map(|_| 0),
        Command::Default(a) => commands::default::run(&deps, &a.spec, a.link).map(|_| 0),
        Command::Alias(a) => commands::alias::set(&deps, &a.name, &a.target).map(|_| 0),
        Command::Unalias(a) => commands::alias::unset(&deps, &a.name).map(|_| 0),
        Command::Pin(a) => commands::pin::run(&deps, &a.spec).map(|_| 0),
        Command::Doctor => commands::doctor::run(&deps),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("cuvm: {e:#}");
            1
        }
    }
}
```

   Extend `crates/cuvm-cli/src/composition.rs` to expose the full dep set (the WU-1 root already builds `inventory`/`resolver`/`activator` via `cuvm_platform::new_activator`; add the compat engine + driver probe + the resolved `Os`/home dir):

```rust
use std::path::PathBuf;

use cuvm_app::{CompatEngine, DriverProbe, Inventory, Resolver};
use cuvm_core::Os;

/// Concrete, fully-wired dependencies. The only place that knows concrete types.
pub struct Deps {
    pub home: PathBuf,
    pub os: Os,
    pub inventory: Box<dyn Inventory>,
    pub resolver: Box<dyn Resolver>,
    pub activator: Box<dyn cuvm_app::Activator>,
    pub compat: Box<dyn CompatEngine>,
    pub driver: Box<dyn DriverProbe>,
}

pub fn build() -> Deps {
    let home = cuvm_home();
    let os = host_os();
    let inventory = cuvm_store::new_inventory(home.clone());
    let resolver = cuvm_store::new_resolver(home.clone());
    let activator = cuvm_platform::new_activator(os);
    let compat = cuvm_core::new_compat_engine();
    let driver = cuvm_nvidia::new_driver_probe();
    Deps { home, os, inventory, resolver, activator, compat, driver }
}

pub fn cuvm_home() -> PathBuf {
    if let Ok(h) = std::env::var("CUVM_HOME") {
        return PathBuf::from(h);
    }
    #[cfg(unix)]
    {
        let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join(".cuvm")
    }
    #[cfg(windows)]
    {
        let base = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join(".cuvm")
    }
}

fn host_os() -> Os {
    #[cfg(windows)]
    {
        Os::Windows
    }
    #[cfg(not(windows))]
    {
        Os::Linux
    }
}
```

   Note: `cuvm_store::new_inventory/new_resolver`, `cuvm_core::new_compat_engine`, and `cuvm_nvidia::new_driver_probe` are the WU-3/WU-7/WU-1 factory functions. If a gating WU exposed a different constructor name, adjust this one call site only (composition root).

   Add empty stub bodies to each `commands/*.rs` so it compiles now (each replaced in its own task below). Example `crates/cuvm-cli/src/commands/ls.rs`:

```rust
use crate::composition::Deps;

pub fn run(_deps: &Deps) -> anyhow::Result<()> {
    Ok(())
}
```

   Create the analogous one-line stubs for `current.rs` (`run(&Deps)`), `which.rs` (`run(&Deps, &str)`), `r#use.rs` (`run(&Deps, Option<&str>, cuvm_core::Shell)`), `default.rs` (`run(&Deps, &str, bool)`), `alias.rs` (`set(&Deps, &str, &str)` + `unset(&Deps, &str)`), `pin.rs` (`run(&Deps, &str)`), `doctor.rs` (`run(&Deps) -> anyhow::Result<i32>`).

4. - [ ] **Run tests, see pass.** Command: `cargo test -p cuvm-cli --test m1_e2e help_lists_m1_commands`
     Expected: `test result: ok. 1 passed`.

5. - [ ] **Commit.**

```bash
git add crates/cuvm-cli && git commit -m "feat(cli): M1 clap command tree + composition wiring (compat + driver deps)" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.6 — `which` returns the absolute toolkit root

**Files:**
- Modify: `crates/cuvm-cli/src/commands/which.rs`
- Test: `crates/cuvm-cli/tests/m1_e2e.rs`

`which <spec>` resolves the spec via `Resolver` and prints `bundle.toolkit.root` as an absolute path on stdout. Unknown spec → nonzero exit + stderr diagnostic.

1. - [ ] **Write the failing test.** Uses a `CUVM_HOME` tempdir seeded with one adopted bundle via the binary's own `adopt` is M2; for M1 we seed the manifest directly using the WU-3 store helper through the binary's env. Simpler: drive `which` against a manifest written by `assert_fs`, since the manifest format is the WU-3 contract.

```rust
use assert_fs::prelude::*;
use assert_fs::TempDir;

fn seed_home_with_bundle(home: &TempDir, version: &str, root: &str) {
    // WU-3 manifest schema (BundleRecord). Adopted source => referenced in place.
    let manifest = format!(
        r#"{{
  "schema_version": 1,
  "bundles": [
    {{"version": "{version}", "source": "Adopted", "path": "{root}",
      "cudnn": null, "components": ["cuda_nvcc","cuda_cudart"],
      "sha256": null, "installed_at": "2026-06-08T00:00:00Z"}}
  ],
  "aliases": {{}},
  "pins": {{}},
  "last_driver": null
}}"#
    );
    home.child("manifest.json").write_str(&manifest).unwrap();
}

#[test]
fn which_prints_absolute_root() {
    let home = TempDir::new().unwrap();
    let tk_root = home.child("versions").child("12.4.1");
    tk_root.create_dir_all().unwrap();
    let abs = tk_root.path().to_string_lossy().replace('\\', "\\\\");
    seed_home_with_bundle(&home, "12.4.1", &abs);

    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["which", "12.4.1"])
        .assert()
        .success()
        .stdout(predicates::str::starts_with("/").or(predicates::str::contains(":\\")))
        .stdout(predicates::str::contains("12.4.1"));
}

#[test]
fn which_unknown_spec_errors() {
    let home = TempDir::new().unwrap();
    seed_home_with_bundle(&home, "12.4.1", &home.path().join("versions/12.4.1").to_string_lossy());
    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["which", "13.0.0"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("cuvm:"));
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-cli --test m1_e2e which_`
     Expected: fail — `which` stub prints nothing, so `stdout(contains("12.4.1"))` fails; the unknown-spec case "succeeds" wrongly.

3. - [ ] **Minimal implementation.** `crates/cuvm-cli/src/commands/which.rs`:

```rust
use std::path::Path;

use crate::composition::Deps;

pub fn run(deps: &Deps, spec: &str) -> anyhow::Result<()> {
    let resolved = deps.resolver.resolve(spec)?;
    let root: &Path = &resolved.bundle.toolkit.root;
    let abs = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    println!("{}", abs.display());
    Ok(())
}
```

4. - [ ] **Run tests, see pass.** Command: `cargo test -p cuvm-cli --test m1_e2e which_`
     Expected: `test result: ok. 2 passed`.

5. - [ ] **Commit.**

```bash
git add crates/cuvm-cli && git commit -m "feat(cli): which resolves spec and prints absolute toolkit root" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.7 — `ls` and `current`

**Files:**
- Modify: `crates/cuvm-cli/src/commands/ls.rs`, `crates/cuvm-cli/src/commands/current.rs`
- Test: `crates/cuvm-cli/tests/m1_e2e.rs`

`ls` lists installed bundle handles (one per line; mark the default alias target with `*`). `current` prints the handle of the active bundle, read from the `CUVM_CURRENT` breadcrumb if set, else the resolved-from-dir bundle, else `none`.

1. - [ ] **Write the failing test.**

```rust
#[test]
fn ls_lists_handles_and_marks_default() {
    let home = TempDir::new().unwrap();
    home.child("versions/12.4.1").create_dir_all().unwrap();
    home.child("versions/12.6.0").create_dir_all().unwrap();
    let manifest = format!(
        r#"{{"schema_version":1,
  "bundles":[
    {{"version":"12.4.1","source":"Adopted","path":"{r1}","cudnn":null,"components":[],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}},
    {{"version":"12.6.0","source":"Adopted","path":"{r2}","cudnn":null,"components":[],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}}
  ],
  "aliases":{{"default":"12.6.0"}},"pins":{{}},"last_driver":null}}"#,
        r1 = home.path().join("versions/12.4.1").to_string_lossy().replace('\\', "\\\\"),
        r2 = home.path().join("versions/12.6.0").to_string_lossy().replace('\\', "\\\\"),
    );
    home.child("manifest.json").write_str(&manifest).unwrap();

    cuvm()
        .env("CUVM_HOME", home.path())
        .arg("ls")
        .assert()
        .success()
        .stdout(predicates::str::contains("12.4.1"))
        .stdout(predicates::str::contains("12.6.0 *").or(predicates::str::contains("* 12.6.0")));
}

#[test]
fn current_reads_breadcrumb() {
    let home = TempDir::new().unwrap();
    home.child("manifest.json")
        .write_str(r#"{"schema_version":1,"bundles":[],"aliases":{},"pins":{},"last_driver":null}"#)
        .unwrap();
    cuvm()
        .env("CUVM_HOME", home.path())
        .env("CUVM_CURRENT", "12.4.1")
        .arg("current")
        .assert()
        .success()
        .stdout(predicates::str::contains("12.4.1"));
}

#[test]
fn current_none_when_no_breadcrumb_and_no_pin() {
    let home = TempDir::new().unwrap();
    home.child("manifest.json")
        .write_str(r#"{"schema_version":1,"bundles":[],"aliases":{},"pins":{},"last_driver":null}"#)
        .unwrap();
    cuvm()
        .env("CUVM_HOME", home.path())
        .env_remove("CUVM_CURRENT")
        .arg("current")
        .assert()
        .success()
        .stdout(predicates::str::contains("none"));
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-cli --test m1_e2e -- ls_ current_`
     Expected: fail — stubs print nothing; `contains("12.4.1")` / `contains("none")` fail.

3. - [ ] **Minimal implementation.** `crates/cuvm-cli/src/commands/ls.rs`:

```rust
use crate::composition::Deps;

pub fn run(deps: &Deps) -> anyhow::Result<()> {
    let manifest = deps.inventory.load()?;
    let default = manifest.aliases.get("default").cloned();
    let bundles = deps.inventory.list()?;
    if bundles.is_empty() {
        println!("(no toolkits installed)");
        return Ok(());
    }
    for b in &bundles {
        let handle = b.handle();
        if default.as_deref() == Some(handle.as_str()) {
            println!("{handle} *");
        } else {
            println!("{handle}");
        }
    }
    Ok(())
}
```

   `crates/cuvm-cli/src/commands/current.rs`:

```rust
use crate::composition::Deps;

pub fn run(deps: &Deps) -> anyhow::Result<()> {
    if let Ok(cur) = std::env::var("CUVM_CURRENT") {
        if !cur.is_empty() {
            println!("{cur}");
            return Ok(());
        }
    }
    let cwd = std::env::current_dir()?;
    match deps.resolver.resolve_from_dir(&cwd)? {
        Some(resolved) => println!("{}", resolved.bundle.handle()),
        None => println!("none"),
    }
    Ok(())
}
```

4. - [ ] **Run tests, see pass.** Command: `cargo test -p cuvm-cli --test m1_e2e -- ls_ current_`
     Expected: `test result: ok. 3 passed`.

5. - [ ] **Commit.**

```bash
git add crates/cuvm-cli && git commit -m "feat(cli): ls lists handles (marks default) and current reads breadcrumb/pin" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.8 — `use` emits the env script via the runtime Activator

**Files:**
- Modify: `crates/cuvm-cli/src/commands/use.rs` (file is `r#use.rs`)
- Test: `crates/cuvm-cli/tests/m1_e2e.rs`

`use [<spec>] --shell <s>` resolves the spec (or resolves-from-dir when omitted), then prints `activator.emit_env(&bundle, shell)` to stdout — the print-then-eval contract (§8). All diagnostics go to stderr. The exact script body is golden-tested in WU-5; here we assert the load-bearing lines reach stdout through the wiring.

1. - [ ] **Write the failing test.**

```rust
#[test]
fn use_emits_bash_env_to_stdout() {
    let home = TempDir::new().unwrap();
    let root = home.child("versions/12.4.1");
    root.create_dir_all().unwrap();
    let abs = root.path().to_string_lossy().replace('\\', "\\\\");
    let manifest = format!(
        r#"{{"schema_version":1,"bundles":[
          {{"version":"12.4.1","source":"Adopted","path":"{abs}","cudnn":null,"components":[],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}}
        ],"aliases":{{}},"pins":{{}},"last_driver":null}}"#
    );
    home.child("manifest.json").write_str(&manifest).unwrap();

    cuvm()
        .env("CUVM_HOME", home.path())
        .args(["use", "12.4.1", "--shell", "bash"])
        .assert()
        .success()
        .stdout(predicates::str::contains("export CUDA_HOME="))
        .stdout(predicates::str::contains("export CUVM_CURRENT=\"12.4.1\""))
        .stdout(predicates::str::contains("CUVM_INJECTED"));
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-cli --test m1_e2e use_emits_bash_env_to_stdout`
     Expected: fail — stub prints nothing; `contains("export CUDA_HOME=")` fails.

3. - [ ] **Minimal implementation.** `crates/cuvm-cli/src/commands/use.rs`:

```rust
use cuvm_core::Shell;

use crate::composition::Deps;

pub fn run(deps: &Deps, spec: Option<&str>, shell: Shell) -> anyhow::Result<()> {
    if !deps.activator.supports(shell) {
        anyhow::bail!("shell {shell:?} is not supported for activation");
    }
    let resolved = match spec {
        Some(s) => deps.resolver.resolve(s)?,
        None => {
            let cwd = std::env::current_dir()?;
            deps.resolver
                .resolve_from_dir(&cwd)?
                .ok_or_else(|| anyhow::anyhow!("no spec given and no .cuda-version / default found"))?
        }
    };
    eprintln!("cuvm: activating {} ({:?})", resolved.bundle.handle(), resolved.via);
    let script = deps.activator.emit_env(&resolved.bundle, shell)?;
    print!("{script}");
    Ok(())
}
```

4. - [ ] **Run tests, see pass.** Command: `cargo test -p cuvm-cli --test m1_e2e use_emits_bash_env_to_stdout`
     Expected: `test result: ok. 1 passed`.

5. - [ ] **Commit.**

```bash
git add crates/cuvm-cli && git commit -m "feat(cli): use resolves spec and emits env script via runtime Activator" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.9 — `alias` / `unalias`

**Files:**
- Modify: `crates/cuvm-cli/src/commands/alias.rs`
- Test: `crates/cuvm-cli/tests/m1_e2e.rs`

`alias <name> <target>` writes the alias via `Inventory::set_alias` (atomic save). `unalias <name>` loads the manifest, removes the key, and saves. Removing a missing alias is a nonzero error.

1. - [ ] **Write the failing test.**

```rust
#[test]
fn alias_then_resolvable_then_unalias() {
    let home = TempDir::new().unwrap();
    let root = home.child("versions/12.4.1");
    root.create_dir_all().unwrap();
    let abs = root.path().to_string_lossy().replace('\\', "\\\\");
    let manifest = format!(
        r#"{{"schema_version":1,"bundles":[
          {{"version":"12.4.1","source":"Adopted","path":"{abs}","cudnn":null,"components":[],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}}
        ],"aliases":{{}},"pins":{{}},"last_driver":null}}"#
    );
    home.child("manifest.json").write_str(&manifest).unwrap();

    cuvm().env("CUVM_HOME", home.path())
        .args(["alias", "prod", "12.4.1"]).assert().success();

    // alias now resolves to the bundle
    cuvm().env("CUVM_HOME", home.path())
        .args(["which", "prod"]).assert().success()
        .stdout(predicates::str::contains("12.4.1"));

    cuvm().env("CUVM_HOME", home.path())
        .args(["unalias", "prod"]).assert().success();

    cuvm().env("CUVM_HOME", home.path())
        .args(["which", "prod"]).assert().failure();
}

#[test]
fn unalias_missing_errors() {
    let home = TempDir::new().unwrap();
    home.child("manifest.json")
        .write_str(r#"{"schema_version":1,"bundles":[],"aliases":{},"pins":{},"last_driver":null}"#)
        .unwrap();
    cuvm().env("CUVM_HOME", home.path())
        .args(["unalias", "ghost"]).assert().failure()
        .stderr(predicates::str::contains("ghost"));
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-cli --test m1_e2e -- alias_ unalias_`
     Expected: fail — stubs no-op; the post-alias `which prod` fails, and `unalias ghost` wrongly succeeds.

3. - [ ] **Minimal implementation.** `crates/cuvm-cli/src/commands/alias.rs`:

```rust
use crate::composition::Deps;

pub fn set(deps: &Deps, name: &str, target: &str) -> anyhow::Result<()> {
    deps.inventory.set_alias(name, target)?;
    eprintln!("cuvm: alias {name} -> {target}");
    Ok(())
}

pub fn unset(deps: &Deps, name: &str) -> anyhow::Result<()> {
    let mut manifest = deps.inventory.load()?;
    if manifest.aliases.remove(name).is_none() {
        anyhow::bail!("no such alias: {name}");
    }
    deps.inventory.save(&manifest)?;
    eprintln!("cuvm: removed alias {name}");
    Ok(())
}
```

4. - [ ] **Run tests, see pass.** Command: `cargo test -p cuvm-cli --test m1_e2e -- alias_ unalias_`
     Expected: `test result: ok. 2 passed`.

5. - [ ] **Commit.**

```bash
git add crates/cuvm-cli && git commit -m "feat(cli): alias sets via Inventory, unalias removes (errors on missing)" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.10 — `default` writes the `default` alias + opt-in `current` symlink

**Files:**
- Modify: `crates/cuvm-cli/src/commands/default.rs`
- Test: `crates/cuvm-cli/tests/m1_e2e.rs`

`default <spec> [--link]` first validates the spec resolves, then writes alias `default → <resolved handle>` (the persistent default per ADR-002). With `--link` it also creates the `~/.cuvm/current` pointer to `versions/<handle>`. The symlink is `#[cfg(unix)]`; on non-unix it is a no-op stub (Windows junction is WU-9). Per §6 the pointer is opt-in.

1. - [ ] **Write the failing test.** Two cases: default without `--link` writes the alias and creates no pointer; default with `--link` creates an absolute symlink (unix lane).

```rust
#[test]
fn default_writes_alias_without_link_by_default() {
    let home = TempDir::new().unwrap();
    let root = home.child("versions/12.6.0");
    root.create_dir_all().unwrap();
    let abs = root.path().to_string_lossy().replace('\\', "\\\\");
    let manifest = format!(
        r#"{{"schema_version":1,"bundles":[
          {{"version":"12.6.0","source":"Adopted","path":"{abs}","cudnn":null,"components":[],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}}
        ],"aliases":{{}},"pins":{{}},"last_driver":null}}"#
    );
    home.child("manifest.json").write_str(&manifest).unwrap();

    cuvm().env("CUVM_HOME", home.path())
        .args(["default", "12.6.0"]).assert().success();

    // alias persisted
    let written = std::fs::read_to_string(home.path().join("manifest.json")).unwrap();
    assert!(written.contains("\"default\""), "{written}");
    assert!(written.contains("12.6.0"));
    // no pointer created
    assert!(!home.path().join("current").exists());
}

#[cfg(unix)]
#[test]
fn default_with_link_creates_current_symlink() {
    let home = TempDir::new().unwrap();
    let root = home.child("versions/12.6.0");
    root.create_dir_all().unwrap();
    let abs = root.path().to_string_lossy();
    let manifest = format!(
        r#"{{"schema_version":1,"bundles":[
          {{"version":"12.6.0","source":"Adopted","path":"{abs}","cudnn":null,"components":[],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}}
        ],"aliases":{{}},"pins":{{}},"last_driver":null}}"#,
        abs = abs.replace('\\', "\\\\")
    );
    home.child("manifest.json").write_str(&manifest).unwrap();

    cuvm().env("CUVM_HOME", home.path())
        .args(["default", "12.6.0", "--link"]).assert().success();

    let ptr = home.path().join("current");
    let target = std::fs::read_link(&ptr).expect("current is a symlink");
    assert!(target.is_absolute());
    assert!(target.ends_with("versions/12.6.0"));
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-cli --test m1_e2e default_`
     Expected: fail — stub does nothing; manifest lacks `"default"`, and the symlink is absent.

3. - [ ] **Minimal implementation.** `crates/cuvm-cli/src/commands/default.rs`:

```rust
use std::path::Path;

use crate::composition::Deps;

pub fn run(deps: &Deps, spec: &str, link: bool) -> anyhow::Result<()> {
    // Validate it actually resolves before persisting.
    let resolved = deps.resolver.resolve(spec)?;
    let handle = resolved.bundle.handle();
    deps.inventory.set_alias("default", &handle)?;
    eprintln!("cuvm: default -> {handle}");

    if link {
        let target = deps.home.join("versions").join(&handle);
        let pointer = deps.home.join("current");
        repoint_current(&pointer, &target)?;
        eprintln!("cuvm: current -> {}", target.display());
    }
    Ok(())
}

#[cfg(unix)]
fn repoint_current(pointer: &Path, target: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::symlink;
    // Re-point atomically: remove any existing pointer first (file or symlink).
    if pointer.symlink_metadata().is_ok() {
        std::fs::remove_file(pointer)?;
    }
    let abs = if target.is_absolute() {
        target.to_path_buf()
    } else {
        std::env::current_dir()?.join(target)
    };
    symlink(&abs, pointer)?;
    Ok(())
}

#[cfg(not(unix))]
fn repoint_current(_pointer: &Path, _target: &Path) -> anyhow::Result<()> {
    // Windows junction repoint is implemented in WU-9 (mklink /J, no admin).
    anyhow::bail!("--link is implemented on the windows lane in WU-9");
}
```

4. - [ ] **Run tests, see pass.** Command (unix lane): `cargo test -p cuvm-cli --test m1_e2e default_`
     Expected: `test result: ok. 2 passed`. On the windows CI lane the `#[cfg(unix)]` symlink test is skipped and the no-link test still passes; the `repoint_current` stub keeps the build green.

5. - [ ] **Commit.**

```bash
git add crates/cuvm-cli && git commit -m "feat(cli): default writes 'default' alias + opt-in current symlink (unix); windows stub" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.11 — `pin` writes `.cuda-version`

**Files:**
- Modify: `crates/cuvm-cli/src/commands/pin.rs`
- Test: `crates/cuvm-cli/tests/m1_e2e.rs`

`pin <spec>` validates the spec resolves, then writes the spec verbatim (newline-terminated) to `.cuda-version` in the current working directory. The pin file name is the contract `.cuda-version`.

1. - [ ] **Write the failing test.**

```rust
#[test]
fn pin_writes_cuda_version_file() {
    let home = TempDir::new().unwrap();
    let root = home.child("versions/12.4.1");
    root.create_dir_all().unwrap();
    let abs = root.path().to_string_lossy().replace('\\', "\\\\");
    let manifest = format!(
        r#"{{"schema_version":1,"bundles":[
          {{"version":"12.4.1","source":"Adopted","path":"{abs}","cudnn":null,"components":[],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}}
        ],"aliases":{{}},"pins":{{}},"last_driver":null}}"#
    );
    home.child("manifest.json").write_str(&manifest).unwrap();

    let proj = TempDir::new().unwrap();
    cuvm()
        .env("CUVM_HOME", home.path())
        .current_dir(proj.path())
        .args(["pin", "12.4"])
        .assert()
        .success();

    let pin = std::fs::read_to_string(proj.path().join(".cuda-version")).unwrap();
    assert_eq!(pin, "12.4\n");
}

#[test]
fn pin_unresolvable_errors_and_writes_nothing() {
    let home = TempDir::new().unwrap();
    home.child("manifest.json")
        .write_str(r#"{"schema_version":1,"bundles":[],"aliases":{},"pins":{},"last_driver":null}"#)
        .unwrap();
    let proj = TempDir::new().unwrap();
    cuvm()
        .env("CUVM_HOME", home.path())
        .current_dir(proj.path())
        .args(["pin", "99.9"])
        .assert()
        .failure();
    assert!(!proj.path().join(".cuda-version").exists());
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-cli --test m1_e2e pin_`
     Expected: fail — stub writes nothing; `read_to_string(".cuda-version")` errors.

3. - [ ] **Minimal implementation.** `crates/cuvm-cli/src/commands/pin.rs`:

```rust
use crate::composition::Deps;

pub fn run(deps: &Deps, spec: &str) -> anyhow::Result<()> {
    // Validate before writing so we never pin an unresolvable spec.
    deps.resolver.resolve(spec)?;
    let cwd = std::env::current_dir()?;
    let file = cwd.join(".cuda-version");
    std::fs::write(&file, format!("{spec}\n"))?;
    eprintln!("cuvm: pinned {spec} in {}", file.display());
    Ok(())
}
```

4. - [ ] **Run tests, see pass.** Command: `cargo test -p cuvm-cli --test m1_e2e pin_`
     Expected: `test result: ok. 2 passed`.

5. - [ ] **Commit.**

```bash
git add crates/cuvm-cli && git commit -m "feat(cli): pin validates spec and writes .cuda-version in cwd" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.12 — `doctor` CLI: read env, probe driver, run use-case, exit with the code

**Files:**
- Modify: `crates/cuvm-cli/src/commands/doctor.rs`
- Test: `crates/cuvm-cli/tests/m1_e2e.rs`

The `doctor` command reads the live env (`PATH`, `LD_LIBRARY_PATH`, `CUDA_HOME`, `CUVM_CURRENT`), probes the driver (`DriverProbe`, graceful-absent), resolves the active bundle (from `CUVM_CURRENT` → manifest, else `resolve_from_dir`), builds `EnvSnapshot`, calls `cuvm_app::doctor::run_doctor`, prints the report, and returns `report.exit_code()`. The broken-PATH path yields exit 2.

1. - [ ] **Write the failing test.** Drive `doctor` with a deliberately broken PATH (dup CUDA + nvcc mismatch) and assert the exact diagnostic codes on stdout + nonzero exit. Force the driver-absent path (`CUVM_FAKE_NO_DRIVER`) so the test is host-independent — the WU-1/WU-7 probe and compat engine honor that test hook (set in composition for the absent case) OR we rely on the real probe degrading gracefully; here we assert only the hygiene findings + exit code, which do not depend on a GPU.

```rust
#[test]
fn doctor_broken_path_reports_mismatch_and_nonzero_exit() {
    let home = TempDir::new().unwrap();
    let active = home.child("versions/12.4.1");
    active.create_dir_all().unwrap();
    let abs = active.path().to_string_lossy().to_string();
    let manifest = format!(
        r#"{{"schema_version":1,"bundles":[
          {{"version":"12.4.1","source":"Adopted","path":"{}","cudnn":null,"components":[],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}}
        ],"aliases":{{}},"pins":{{}},"last_driver":null}}"#,
        abs.replace('\\', "\\\\")
    );
    home.child("manifest.json").write_str(&manifest).unwrap();

    // stale /opt/cuda-12.2/bin FIRST on PATH, CUDA_HOME = active 12.4.1 -> nvcc mismatch + dup
    let broken_path = format!("/opt/cuda-12.2/bin:{}/bin:/usr/bin", abs);
    let broken_ld = format!("/opt/cuda-12.2/lib64:{}/lib64", abs);

    cuvm()
        .env("CUVM_HOME", home.path())
        .env("CUVM_CURRENT", "12.4.1")
        .env("PATH", &broken_path)
        .env("LD_LIBRARY_PATH", &broken_ld)
        .env("CUDA_HOME", &abs)
        .assert_then(); // placeholder; replaced below
}
```

   Refine the test (the binary must still be locatable even though we override `PATH`; use `assert_cmd`'s `Command::cargo_bin` which records the absolute binary path, so overriding `PATH` is safe):

```rust
#[test]
fn doctor_broken_path_reports_mismatch_and_nonzero_exit() {
    let home = TempDir::new().unwrap();
    let active = home.child("versions/12.4.1");
    active.create_dir_all().unwrap();
    let abs = active.path().to_string_lossy().to_string();
    let manifest = format!(
        r#"{{"schema_version":1,"bundles":[
          {{"version":"12.4.1","source":"Adopted","path":"{}","cudnn":null,"components":[],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}}
        ],"aliases":{{}},"pins":{{}},"last_driver":null}}"#,
        abs.replace('\\', "\\\\")
    );
    home.child("manifest.json").write_str(&manifest).unwrap();

    let broken_path = format!("/opt/cuda-12.2/bin:{abs}/bin:/usr/bin");
    let broken_ld = format!("/opt/cuda-12.2/lib64:{abs}/lib64");

    cuvm()
        .env("CUVM_HOME", home.path())
        .env("CUVM_CURRENT", "12.4.1")
        .env("PATH", &broken_path)
        .env("LD_LIBRARY_PATH", &broken_ld)
        .env("CUDA_HOME", &abs)
        .arg("doctor")
        .assert()
        .code(predicates::ord::ge(2)) // BLOCK present => exit >= 2
        .stdout(predicates::str::contains("NVCC_MISMATCH"))
        .stdout(predicates::str::contains("PATH_DUP_CUDA"))
        .stdout(predicates::str::contains("/opt/cuda-12.2/bin"));
}

#[test]
fn doctor_clean_env_exits_zero_or_warn_not_block() {
    let home = TempDir::new().unwrap();
    let active = home.child("versions/12.4.1");
    active.create_dir_all().unwrap();
    let abs = active.path().to_string_lossy().to_string();
    let manifest = format!(
        r#"{{"schema_version":1,"bundles":[
          {{"version":"12.4.1","source":"Adopted","path":"{}","cudnn":null,"components":[],"sha256":null,"installed_at":"2026-06-08T00:00:00Z"}}
        ],"aliases":{{}},"pins":{{}},"last_driver":null}}"#,
        abs.replace('\\', "\\\\")
    );
    home.child("manifest.json").write_str(&manifest).unwrap();

    cuvm()
        .env("CUVM_HOME", home.path())
        .env("CUVM_CURRENT", "12.4.1")
        .env("PATH", format!("{abs}/bin:/usr/bin"))
        .env("LD_LIBRARY_PATH", format!("{abs}/lib64"))
        .env("CUDA_HOME", &abs)
        .arg("doctor")
        .assert()
        .code(predicates::ord::lt(2)) // no BLOCK
        .stdout(predicates::str::contains("PATH_HYGIENE").or(predicates::str::contains("DRIVER")));
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-cli --test m1_e2e doctor_`
     Expected: fail — `doctor` stub returns `Ok(0)` with no output; `code(ge(2))` and the `NVCC_MISMATCH` substring fail.

3. - [ ] **Minimal implementation.** `crates/cuvm-cli/src/commands/doctor.rs`:

```rust
use cuvm_app::doctor::{run_doctor, EnvSnapshot};
use cuvm_core::Version;

use crate::composition::Deps;

pub fn run(deps: &Deps) -> anyhow::Result<i32> {
    // Probe the driver (graceful-absent: probe returns present=false, never errors hard).
    let driver = deps.driver.probe()?;

    // Determine the active toolkit version: CUVM_CURRENT breadcrumb -> resolve_from_dir.
    let active: Option<Version> = active_version(deps)?;

    let env = EnvSnapshot {
        path: std::env::var("PATH").unwrap_or_default(),
        ld_library_path: std::env::var("LD_LIBRARY_PATH").unwrap_or_default(),
        cuda_home: std::env::var("CUDA_HOME").ok().filter(|s| !s.is_empty()),
        active_root: active_root(deps, active.as_ref())?,
        path_sep: path_sep(),
    };

    let report = run_doctor(deps.compat.as_ref(), &driver, active.as_ref(), &env);
    print!("{report}");
    println!();
    Ok(report.exit_code())
}

fn active_version(deps: &Deps) -> anyhow::Result<Option<Version>> {
    if let Ok(cur) = std::env::var("CUVM_CURRENT") {
        if !cur.is_empty() {
            return Ok(Some(Version::parse(&cur)?));
        }
    }
    let cwd = std::env::current_dir()?;
    match deps.resolver.resolve_from_dir(&cwd)? {
        Some(r) => Ok(Some(r.bundle.toolkit.version.clone())),
        None => Ok(None),
    }
}

fn active_root(deps: &Deps, active: Option<&Version>) -> anyhow::Result<Option<String>> {
    let Some(active) = active else { return Ok(None) };
    let manifest = deps.inventory.load()?;
    for b in &manifest.bundles {
        if b.version == active.raw {
            return Ok(Some(b.path.clone()));
        }
    }
    // Fall back to the conventional downloaded path.
    Ok(Some(
        deps.home
            .join("versions")
            .join(&active.raw)
            .to_string_lossy()
            .into_owned(),
    ))
}

fn path_sep() -> char {
    if cfg!(windows) {
        ';'
    } else {
        ':'
    }
}
```

4. - [ ] **Run tests, see pass.** Command: `cargo test -p cuvm-cli --test m1_e2e doctor_`
     Expected: `test result: ok. 2 passed`.

5. - [ ] **Commit.**

```bash
git add crates/cuvm-cli && git commit -m "feat(cli): doctor reads env, probes driver, runs use-case, exits with machine code" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.13 — End-to-end: adopt 2 fakes → default → use → current

**Files:**
- Test: `crates/cuvm-cli/tests/m1_e2e.rs`

A full M1 black-box flow. M2's real `adopt` (download/scan) is out of scope, but WU-4 (Linux adopt) provides the `adopt` command for adopting an existing `/usr/local/cuda-*`-shaped dir. We adopt two fake toolkit dirs we lay down in a tempdir, point `cuvm` at them via the WU-4 scan path (`--path` flag accepted by the WU-4 `adopt` command), then exercise default → use → current end to end. If WU-4 lands `adopt --scan`/`adopt <path>`, use that; this test asserts only the user-visible result, so it is robust to the exact adopt flag spelling.

1. - [ ] **Write the failing test.**

```rust
fn make_fake_toolkit(dir: &std::path::Path, version: &str) {
    // A minimal adoptable tree: bin/nvcc + lib64 (so adopt records it, has_lib64=true).
    let root = dir.join(format!("cuda-{version}"));
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::create_dir_all(root.join("lib64")).unwrap();
    // a fake nvcc that prints a matching version (adopt/which never executes it in M1)
    std::fs::write(root.join("bin/nvcc"), "#!/bin/sh\necho release {v}\n".replace("{v}", version)).unwrap();
}

#[test]
fn e2e_adopt_two_then_default_use_current() {
    let home = TempDir::new().unwrap();
    let installs = TempDir::new().unwrap();
    make_fake_toolkit(installs.path(), "12.4.1");
    make_fake_toolkit(installs.path(), "12.6.0");

    // adopt both (WU-4 adopt-by-path)
    for v in ["12.4.1", "12.6.0"] {
        let p = installs.path().join(format!("cuda-{v}"));
        cuvm().env("CUVM_HOME", home.path())
            .args(["adopt", p.to_str().unwrap()])
            .assert().success();
    }

    // ls shows both
    cuvm().env("CUVM_HOME", home.path()).arg("ls").assert().success()
        .stdout(predicates::str::contains("12.4.1"))
        .stdout(predicates::str::contains("12.6.0"));

    // default -> 12.6.0
    cuvm().env("CUVM_HOME", home.path())
        .args(["default", "12.6.0"]).assert().success();

    // ls now marks 12.6.0
    cuvm().env("CUVM_HOME", home.path()).arg("ls").assert().success()
        .stdout(predicates::str::contains("12.6.0 *").or(predicates::str::contains("* 12.6.0")));

    // current with no breadcrumb resolves the default
    cuvm().env("CUVM_HOME", home.path())
        .env_remove("CUVM_CURRENT")
        .arg("current").assert().success()
        .stdout(predicates::str::contains("12.6.0"));

    // use 12.4.1 emits an env script referencing the 12.4.1 root
    cuvm().env("CUVM_HOME", home.path())
        .args(["use", "12.4.1", "--shell", "bash"]).assert().success()
        .stdout(predicates::str::contains("export CUVM_CURRENT=\"12.4.1\""))
        .stdout(predicates::str::contains("cuda-12.4.1").or(predicates::str::contains("12.4.1")));
}
```

2. - [ ] **Run it, see it fail.** Command: `cargo test -p cuvm-cli --test m1_e2e e2e_adopt_two_then_default_use_current`
     Expected: fail until all prior tasks are green AND WU-4 `adopt <path>` is present. If the adopt flag spelling differs, the failure surfaces in the adopt step; fix is a one-line arg change in this test only (the gating WU-4 owns the command).

3. - [ ] **Minimal implementation.** No new production code — this test exercises already-built commands. If it fails on adopt-arg mismatch, adjust the `args(["adopt", ...])` call to match the WU-4 surface (e.g. `["adopt", "--path", p]`). No other change.

4. - [ ] **Run tests, see pass.** Command: `cargo test -p cuvm-cli --test m1_e2e e2e_adopt_two_then_default_use_current`
     Expected: `test result: ok. 1 passed`. Then run the full WU-8 suite: `cargo test -p cuvm-app -p cuvm-cli` → Expected: all green.

5. - [ ] **Commit.**

```bash
git add crates/cuvm-cli && git commit -m "test(cli): e2e adopt two fakes -> default -> use -> current" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 8.14 — Workspace-wide green gate + clippy

**Files:**
- (no source change unless lints fire)

Final WU-8 gate: the whole workspace builds, all tests pass, and clippy is clean. This is the M1 ship-candidate checkpoint ① (with WU-9 in parallel).

1. - [ ] **Run the full suite.** Command: `cargo test --workspace`
     Expected: `test result: ok` for every crate; zero failures.

2. - [ ] **Run clippy as an error gate.** Command: `cargo clippy --workspace --all-targets -- -D warnings`
     Expected: `Finished` with no warnings. Fix any lint in place (typically `needless_borrow`/`redundant_clone`), then re-run.

3. - [ ] **Run the doctor snapshot deterministically once more.** Command: `cargo test -p cuvm-app doctor::aggregate_tests` and `cargo insta test --review --package cuvm-app`
     Expected: `0 snapshots to review` (the committed `.snap` matches byte-for-byte).

4. - [ ] **Commit (only if lints required a fix).**

```bash
git add -A && git commit -m "chore(m1): clippy-clean workspace, M1 command set + doctor v1 green" -m "Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

**WU-8 done when:** `cargo test --workspace` is green; `doctor` on the deliberately broken PATH emits the exact `PATH_DUP_CUDA` + `NVCC_MISMATCH` diagnostics and exits with code `2`; `default` writes the `default` alias and (with `--link`) the opt-in `current` symlink; `pin` writes `.cuda-version`; `which` prints an absolute path; and the assert_cmd e2e (`adopt` 2 fakes → `default` → `use` → `current`) passes. Gates satisfied: WU-2 (Resolver), WU-3 (Inventory/Manifest), WU-5 (Activator), WU-7 (CompatEngine). Windows `--link` junction + `default` HKCU broadcast land in WU-9.

---

### WU-9: Windows backend (Activator + adopt + persistence)

Implements the Windows `Activator` + `Installer` (scan/adopt) impls in `cuvm-platform`, the persistent-default machinery (HKCU `Environment` read-modify-write + `WM_SETTINGCHANGE` broadcast + `mklink /J` junction), and the `cuvm.ps1` / `cuvm.cmd` shims with a chained `prompt()` hook in `cuvm-cli`.

**Design rules pinned from the spec (do not re-derive):**
- §8 — PowerShell emitter sets `$env:CUDA_PATH`/`$env:CUDA_HOME`/`$env:CUDAToolkit_ROOT`, prepends `…\bin`, strips the previous `CUVM_INJECTED` segments out of `$env:Path` via `-split ';'`, rewrites the `CUVM_CURRENT`/`CUVM_INJECTED` breadcrumbs. cmd emitter uses `set NAME=VALUE`.
- §2.2 — persistent default uses **HKCU\Environment** REG_EXPAND_SZ read-modify-write (**never** `setx` a constructed PATH — 1024-char truncation), broadcasts `WM_SETTINGCHANGE` with `"Environment"` via `SendMessageTimeout`, and re-points a `mklink /J` **junction** `…\.cuvm\current → versions\vX.Y` (junctions need no admin).
- §2.2 / §9 — adopt scans `CUDA_PATH`, `CUDA_PATH_VX_Y`, and `C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\vX.Y` (read-only, `Source::Adopted`).
- §3 / contract — **script emission is runtime-dispatched** (the `WindowsActivator` compiles and its golden tests run on Linux); only the win32 syscall floor (`persist`, `junction`) is behind `#[cfg(windows)]`, each with a non-windows stub so the crate builds on the gnu/linux host.
- Gates: **shim-protocol** (print-then-eval, `CUVM_INJECTED` cleanup), **compat-data** (Windows column N/A from 13.0 — already a WU-7 regression test; this WU must not emit a Windows toolkit ≥ 13.0 in any golden fixture).

Assumes from earlier WUs: `cuvm-core` types (`Version`, `Os`, `Arch`, `Platform`, `Shell`, `Source`, `Toolkit`, `Bundle`, `EnvPlan`, `VersionMeta`, etc.), the `cuvm-app` ports `Activator`/`Installer` and their structs (`Candidate`, `Cached`, `AcquirePlan`, `ArtifactKind`), and `cuvm_platform::new_activator(os)->Box<dyn Activator>` / `new_installer(os)->Box<dyn Installer>` (WU-1 stubs). `EnvPlan` was built by WU-5 for Linux; WU-9 reuses the same `EnvPlan` shape and renders it for Windows shells.

---

#### Task 9.1 — Add Windows deps + module skeleton to `cuvm-platform`

**Files:**
- Modify: `crates/cuvm-platform/Cargo.toml`
- Create: `crates/cuvm-platform/src/windows/mod.rs`
- Modify: `crates/cuvm-platform/src/lib.rs`

1. - [ ] Step: Add the `windows` crate (target-gated) + dev-deps to `cuvm-platform/Cargo.toml`. The `windows` crate only links on the windows target; the gnu/linux host never pulls it.
```toml
[dependencies]
cuvm-core = { path = "../cuvm-core" }
cuvm-app  = { path = "../cuvm-app" }
anyhow = { workspace = true }

[target.'cfg(windows)'.dependencies]
windows = { workspace = true, features = [
    "Win32_System_Registry",
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
] }

[dev-dependencies]
insta = { workspace = true }
tempfile = { workspace = true }
assert_fs = { workspace = true }
```
2. - [ ] Step: Add the `windows` version to the workspace `[workspace.dependencies]` table in the root `Cargo.toml` (single source of truth):
```toml
windows = "0.58"
```
3. - [ ] Step: Create the module skeleton `crates/cuvm-platform/src/windows/mod.rs`. The two impl modules (`activator`, `installer`) are pure/runtime and always compile; the two syscall modules (`persist`, `junction`) compile everywhere but split their bodies with `#[cfg(windows)]` + stub.
```rust
//! Windows backend: runtime-dispatched script emission (compiles + golden-tests
//! on every host) and a thin win32 syscall floor for persistence + junctions.

pub mod activator;
pub mod installer;
pub mod junction;
pub mod persist;

pub use activator::WindowsActivator;
pub use installer::WindowsInstaller;
```
4. - [ ] Step: Wire the module into `crates/cuvm-platform/src/lib.rs` and extend the runtime factory to return the Windows impls for `Os::Windows` (replacing the WU-1 stub arm). Keep the existing `unix` module + `Os::Linux` arm untouched.
```rust
pub mod unix;
pub mod windows;

use cuvm_app::{Activator, Installer};
use cuvm_core::Os;

pub fn new_activator(os: Os) -> Box<dyn Activator> {
    match os {
        Os::Linux => Box::new(unix::UnixActivator::new()),
        Os::Windows => Box::new(windows::WindowsActivator::new()),
    }
}

pub fn new_installer(os: Os) -> Box<dyn Installer> {
    match os {
        Os::Linux => Box::new(unix::UnixInstaller::new()),
        Os::Windows => Box::new(windows::WindowsInstaller::new()),
    }
}
```
5. - [ ] Step: Run a compile check (the impl files don't exist yet, so this is expected to fail on unresolved imports — confirming the wiring is in place).
```
cargo build -p cuvm-platform
```
Expected: fail — `error[E0583]: file not found for module 'activator'` / unresolved `WindowsActivator`. This proves the factory now points at the new module.
6. - [ ] Step: Commit the skeleton.
```bash
git add crates/cuvm-platform/Cargo.toml Cargo.toml crates/cuvm-platform/src/lib.rs crates/cuvm-platform/src/windows/mod.rs && git commit -m "build(platform): scaffold windows backend module + windows crate dep

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 9.2 — PowerShell `emit_env` (golden, runs on Linux)

**Files:**
- Create: `crates/cuvm-platform/src/windows/activator.rs`
- Test: `crates/cuvm-platform/tests/windows_env_golden.rs`
- Test (snapshot): `crates/cuvm-platform/tests/snapshots/windows_env_golden__powershell_use.snap`

1. - [ ] Step: Write the failing golden test for `emit_env` on `Shell::PowerShell`. The fixture is a Windows toolkit **12.4.1** (must stay < 13.0 per the compat-data gate). The test constructs a `Bundle` via a local helper and snapshots the emitted script.
```rust
// crates/cuvm-platform/tests/windows_env_golden.rs
use cuvm_app::Activator;
use cuvm_core::{
    Arch, Bundle, Os, Platform, Shell, Source, Toolkit, Version,
};
use cuvm_platform::windows::WindowsActivator;
use time::OffsetDateTime;

fn win_bundle() -> Bundle {
    let platform = Platform { os: Os::Windows, arch: Arch::X86_64 };
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
    Bundle { toolkit, cudnn: None, extra: vec![] }
}

#[test]
fn powershell_use() {
    let act = WindowsActivator::new();
    let script = act.emit_env(&win_bundle(), Shell::PowerShell).unwrap();
    insta::assert_snapshot!(script);
}
```
2. - [ ] Step: Run it, see it fail.
```
cargo test -p cuvm-platform --test windows_env_golden powershell_use
```
Expected: fail — compile error `cannot find function 'new' ... WindowsActivator` (impl not written yet).
3. - [ ] Step: Minimal implementation of `WindowsActivator` with PowerShell `emit_env`. The PowerShell `CUVM_INJECTED` strip uses `-split ';'` and rebuilds `$env:Path` excluding the previously-injected segments (per §8); `CUDA_PATH`/`CUDA_HOME`/`CUDAToolkit_ROOT` are all set; `…\bin` is prepended; breadcrumbs are rewritten.
```rust
// crates/cuvm-platform/src/windows/activator.rs
use anyhow::{bail, Result};
use cuvm_app::Activator;
use cuvm_core::{Bundle, Shell};

pub struct WindowsActivator;

impl WindowsActivator {
    pub fn new() -> Self {
        WindowsActivator
    }

    fn ps_env(&self, b: &Bundle) -> String {
        let root = b.toolkit.root.to_string_lossy().replace('/', "\\");
        let bin = format!("{root}\\bin");
        let current = b.toolkit.version.raw.clone();
        let injected = bin.clone();
        format!(
            r#"if ($env:CUVM_INJECTED) {{
  $cuvm_inj = $env:CUVM_INJECTED -split ';'
  $env:Path = (($env:Path -split ';') | Where-Object {{ $_ -and ($cuvm_inj -notcontains $_) }}) -join ';'
}}
$env:CUDA_HOME = '{root}'
$env:CUDA_PATH = '{root}'
$env:CUDAToolkit_ROOT = '{root}'
$env:Path = '{bin};' + $env:Path
$env:CUVM_CURRENT = '{current}'
$env:CUVM_INJECTED = '{injected}'
"#
        )
    }

    fn cmd_env(&self, b: &Bundle) -> String {
        let root = b.toolkit.root.to_string_lossy().replace('/', "\\");
        let bin = format!("{root}\\bin");
        let current = b.toolkit.version.raw.clone();
        format!(
            "set \"CUDA_HOME={root}\"\r\n\
             set \"CUDA_PATH={root}\"\r\n\
             set \"CUDAToolkit_ROOT={root}\"\r\n\
             set \"PATH={bin};%PATH%\"\r\n\
             set \"CUVM_CURRENT={current}\"\r\n\
             set \"CUVM_INJECTED={bin}\"\r\n"
        )
    }
}

impl Activator for WindowsActivator {
    fn emit_env(&self, b: &Bundle, sh: Shell) -> Result<String> {
        match sh {
            Shell::PowerShell => Ok(self.ps_env(b)),
            Shell::Cmd => Ok(self.cmd_env(b)),
            other => bail!("WindowsActivator does not support {other:?}"),
        }
    }

    fn emit_deactivate(&self, _sh: Shell) -> Result<String> {
        unimplemented!("task 9.4")
    }

    fn hook(&self, _sh: Shell) -> Result<String> {
        unimplemented!("task 9.5")
    }

    fn supports(&self, sh: Shell) -> bool {
        matches!(sh, Shell::PowerShell | Shell::Cmd)
    }
}
```
4. - [ ] Step: Run the test to accept the snapshot, then run again to confirm green. (`INSTA_UPDATE=always` writes `windows_env_golden__powershell_use.snap`.)
```
INSTA_UPDATE=always cargo test -p cuvm-platform --test windows_env_golden powershell_use && cargo test -p cuvm-platform --test windows_env_golden powershell_use
```
Expected: `test result: ok. 1 passed`. Review the written `.snap`: `$env:CUDA_PATH/CUDA_HOME/CUDAToolkit_ROOT = 'C:\Users\dev\.cuvm\versions\12.4.1'`, `$env:Path = 'C:\…\12.4.1\bin;' + $env:Path`, strip block uses `-split ';'`, `CUVM_CURRENT = '12.4.1'`.
5. - [ ] Step: Add a regression assertion for the repeated-`use` strip — a second test that the emitted strip block references `CUVM_INJECTED` and `-notcontains` so no PATH duplication can occur. Append to `windows_env_golden.rs`:
```rust
#[test]
fn powershell_strip_is_idempotent() {
    let act = WindowsActivator::new();
    let script = act.emit_env(&win_bundle(), Shell::PowerShell).unwrap();
    assert!(script.contains("$env:CUVM_INJECTED -split ';'"));
    assert!(script.contains("$cuvm_inj -notcontains $_"));
}
```
6. - [ ] Step: Run both PowerShell tests, see pass.
```
cargo test -p cuvm-platform --test windows_env_golden powershell
```
Expected: `test result: ok. 2 passed`.
7. - [ ] Step: Commit.
```bash
git add crates/cuvm-platform/src/windows/activator.rs crates/cuvm-platform/tests/windows_env_golden.rs crates/cuvm-platform/tests/snapshots/windows_env_golden__powershell_use.snap && git commit -m "feat(platform): powershell emit_env with CUVM_INJECTED strip (golden)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 9.3 — cmd `emit_env` (golden, runs on Linux)

**Files:**
- Modify: `crates/cuvm-platform/tests/windows_env_golden.rs`
- Test (snapshot): `crates/cuvm-platform/tests/snapshots/windows_env_golden__cmd_use.snap`

1. - [ ] Step: Write the failing golden test for `Shell::Cmd`. cmd uses CRLF line endings and `set "NAME=VALUE"` form.
```rust
#[test]
fn cmd_use() {
    let act = WindowsActivator::new();
    let script = act.emit_env(&win_bundle(), Shell::Cmd).unwrap();
    insta::assert_snapshot!(script);
}
```
2. - [ ] Step: Run it, see it fail (no accepted snapshot yet).
```
cargo test -p cuvm-platform --test windows_env_golden cmd_use
```
Expected: fail — `cmd_use` reports a new snapshot (`insta` exits non-zero on an unreviewed snapshot).
3. - [ ] Step: Implementation already exists (`cmd_env` was written in Task 9.2). Accept and verify the snapshot.
```
INSTA_UPDATE=always cargo test -p cuvm-platform --test windows_env_golden cmd_use && cargo test -p cuvm-platform --test windows_env_golden cmd_use
```
Expected: `test result: ok. 1 passed`. The `.snap` shows `set "CUDA_PATH=C:\…\12.4.1"`, `set "PATH=C:\…\12.4.1\bin;%PATH%"`, `set "CUVM_INJECTED=C:\…\12.4.1\bin"`.
4. - [ ] Step: Commit.
```bash
git add crates/cuvm-platform/tests/windows_env_golden.rs crates/cuvm-platform/tests/snapshots/windows_env_golden__cmd_use.snap && git commit -m "feat(platform): cmd emit_env set-based env script (golden)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 9.4 — `emit_deactivate` (PowerShell, golden)

**Files:**
- Modify: `crates/cuvm-platform/src/windows/activator.rs`
- Modify: `crates/cuvm-platform/tests/windows_env_golden.rs`
- Test (snapshot): `crates/cuvm-platform/tests/snapshots/windows_env_golden__powershell_deactivate.snap`

1. - [ ] Step: Write the failing test — deactivate must strip the injected segments out of `$env:Path` and `Remove-Item` the CUDA env vars + breadcrumbs.
```rust
#[test]
fn powershell_deactivate() {
    let act = WindowsActivator::new();
    let script = act.emit_deactivate(Shell::PowerShell).unwrap();
    insta::assert_snapshot!(script);
}
```
2. - [ ] Step: Run it, see it fail.
```
cargo test -p cuvm-platform --test windows_env_golden powershell_deactivate
```
Expected: fail — panics at `unimplemented!("task 9.4")`.
3. - [ ] Step: Implement `emit_deactivate`. Replace the `unimplemented!` body.
```rust
    fn emit_deactivate(&self, sh: Shell) -> Result<String> {
        match sh {
            Shell::PowerShell => Ok(r#"if ($env:CUVM_INJECTED) {
  $cuvm_inj = $env:CUVM_INJECTED -split ';'
  $env:Path = (($env:Path -split ';') | Where-Object { $_ -and ($cuvm_inj -notcontains $_) }) -join ';'
}
Remove-Item Env:\CUDA_HOME -ErrorAction SilentlyContinue
Remove-Item Env:\CUDA_PATH -ErrorAction SilentlyContinue
Remove-Item Env:\CUDAToolkit_ROOT -ErrorAction SilentlyContinue
Remove-Item Env:\CUVM_CURRENT -ErrorAction SilentlyContinue
Remove-Item Env:\CUVM_INJECTED -ErrorAction SilentlyContinue
"#
            .to_string()),
            Shell::Cmd => Ok("set \"CUDA_HOME=\"\r\nset \"CUDA_PATH=\"\r\n\
                              set \"CUDAToolkit_ROOT=\"\r\nset \"CUVM_CURRENT=\"\r\n\
                              set \"CUVM_INJECTED=\"\r\n"
                .to_string()),
            other => bail!("WindowsActivator does not support {other:?}"),
        }
    }
```
4. - [ ] Step: Accept + verify the snapshot.
```
INSTA_UPDATE=always cargo test -p cuvm-platform --test windows_env_golden powershell_deactivate && cargo test -p cuvm-platform --test windows_env_golden powershell_deactivate
```
Expected: `test result: ok. 1 passed`. Snapshot strips `CUVM_INJECTED` from `$env:Path`, then `Remove-Item` each var.
5. - [ ] Step: Commit.
```bash
git add crates/cuvm-platform/src/windows/activator.rs crates/cuvm-platform/tests/windows_env_golden.rs crates/cuvm-platform/tests/snapshots/windows_env_golden__powershell_deactivate.snap && git commit -m "feat(platform): windows emit_deactivate strips injected path + vars

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 9.5 — `hook` — chained PowerShell `prompt()` + cmd "unsupported" warning (golden)

**Files:**
- Modify: `crates/cuvm-platform/src/windows/activator.rs`
- Modify: `crates/cuvm-platform/tests/windows_env_golden.rs`
- Test (snapshot): `crates/cuvm-platform/tests/snapshots/windows_env_golden__powershell_hook.snap`

1. - [ ] Step: Write the failing test. The PowerShell hook must **chain** any existing `prompt` (oh-my-posh/Starship) per §2.5 — capture the prior function and call it. cmd has no cd-hook (§2.5) → warn-only on stderr handled by caller; `hook` returns an empty body for cmd.
```rust
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
```
2. - [ ] Step: Run them, see fail.
```
cargo test -p cuvm-platform --test windows_env_golden hook
```
Expected: fail — both panic at `unimplemented!("task 9.5")`.
3. - [ ] Step: Implement `hook`. The PowerShell hook stashes the existing `prompt` into `$global:__cuvm_prev_prompt` (once) and defines a new `prompt` that runs `cuvm use` for any `.cuda-version` change then calls the stashed prompt.
```rust
    fn hook(&self, sh: Shell) -> Result<String> {
        match sh {
            Shell::PowerShell => Ok(r#"if (-not (Test-Path Variable:\__cuvm_prev_prompt)) {
  $cmd = Get-Command prompt -ErrorAction SilentlyContinue
  if ($cmd) { $global:__cuvm_prev_prompt = $cmd.ScriptBlock }
}
function global:prompt {
  try { (& cuvm.exe use --shell powershell --quiet | Out-String) | Invoke-Expression } catch {}
  if ($global:__cuvm_prev_prompt) { & $global:__cuvm_prev_prompt } else { "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) " }
}
"#
            .to_string()),
            Shell::Cmd => Ok(String::new()),
            other => bail!("WindowsActivator does not support {other:?}"),
        }
    }
```
4. - [ ] Step: Accept + verify the snapshot and assertions.
```
INSTA_UPDATE=always cargo test -p cuvm-platform --test windows_env_golden hook && cargo test -p cuvm-platform --test windows_env_golden hook
```
Expected: `test result: ok. 2 passed`. Snapshot stashes the prior prompt once and re-invokes it at the end (chaining).
5. - [ ] Step: Run the whole activator golden suite to confirm nothing regressed.
```
cargo test -p cuvm-platform --test windows_env_golden
```
Expected: `test result: ok. 7 passed`.
6. - [ ] Step: Commit.
```bash
git add crates/cuvm-platform/src/windows/activator.rs crates/cuvm-platform/tests/windows_env_golden.rs crates/cuvm-platform/tests/snapshots/windows_env_golden__powershell_hook.snap && git commit -m "feat(platform): chained powershell prompt() hook; cmd no-op hook

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 9.6 — Windows `Installer::scan` + `adopt` (fixture tree, runs on Linux)

**Files:**
- Create: `crates/cuvm-platform/src/windows/installer.rs`
- Test: `crates/cuvm-platform/tests/windows_adopt.rs`

Note: `scan`/`adopt` are pure filesystem walks parameterized by their search roots, so they run on the Linux host against a `tempfile` fixture tree. The real defaults (`C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v*`, `CUDA_PATH`, `CUDA_PATH_VX_Y`) are injected via a `roots: Vec<PathBuf>` field so production reads env on Windows and tests point at a temp dir.

1. - [ ] Step: Write the failing test. Build a fixture mimicking `…\CUDA\v12.4` with a `bin\nvcc.exe` and a `version.json`, then assert `scan` finds one candidate at `v12.4` and `adopt` returns a `Bundle` with `Source::Adopted`, version `12.4`, and the fixture root.
```rust
// crates/cuvm-platform/tests/windows_adopt.rs
use assert_fs::prelude::*;
use cuvm_app::Installer;
use cuvm_core::Source;
use cuvm_platform::windows::WindowsInstaller;

#[test]
fn scan_finds_and_adopts_program_files_install() {
    let tmp = assert_fs::TempDir::new().unwrap();
    // Mimic: <root>\CUDA\v12.4\bin\nvcc.exe
    tmp.child("CUDA/v12.4/bin/nvcc.exe").write_str("").unwrap();
    tmp.child("CUDA/v12.4/lib/x64/cudart.lib").write_str("").unwrap();
    // A non-version dir must be ignored.
    tmp.child("CUDA/extras/readme.txt").write_str("").unwrap();

    let installer = WindowsInstaller::with_roots(vec![tmp.child("CUDA").path().to_path_buf()]);

    let cands = installer.scan().unwrap();
    assert_eq!(cands.len(), 1, "expected exactly one versioned toolkit dir");
    assert_eq!(cands[0].version.raw, "12.4");

    let bundle = installer.adopt(&cands[0]).unwrap();
    assert_eq!(bundle.toolkit.source, Source::Adopted);
    assert_eq!(bundle.toolkit.version.raw, "12.4");
    assert_eq!(bundle.toolkit.root, tmp.child("CUDA/v12.4").path());
    assert!(bundle.toolkit.platform.os == cuvm_core::Os::Windows);
}
```
2. - [ ] Step: Run it, see fail.
```
cargo test -p cuvm-platform --test windows_adopt
```
Expected: fail — `cannot find function 'with_roots' ... WindowsInstaller`.
3. - [ ] Step: Implement the installer. `scan` walks each root for `v<MAJOR>.<MINOR>` dirs containing `bin\nvcc.exe`; `adopt` reads the dir into an `Adopted` `Toolkit`/`Bundle`. The default constructor reads `CUDA_PATH`, every `CUDA_PATH_V*` env var, and the standard Program Files root. The acquire/extract/place/verify/smoke methods are not part of WU-9 (they land in WU-14) — keep them `unimplemented!` here.
```rust
// crates/cuvm-platform/src/windows/installer.rs
use std::path::PathBuf;

use anyhow::{Context, Result};
use cuvm_app::{AcquirePlan, ArtifactKind, Cached, Candidate, Installer};
use cuvm_core::{Arch, Bundle, Os, Platform, Source, Toolkit, Version, VersionMeta};
use time::OffsetDateTime;

pub struct WindowsInstaller {
    roots: Vec<PathBuf>,
}

impl WindowsInstaller {
    pub fn new() -> Self {
        WindowsInstaller { roots: default_roots() }
    }

    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        WindowsInstaller { roots }
    }

    fn windows_platform() -> Platform {
        Platform { os: Os::Windows, arch: Arch::X86_64 }
    }
}

/// Default scan roots per spec §2.2: Program Files install dir + CUDA_PATH +
/// every CUDA_PATH_VX_Y. Reading real env is host-neutral (empty on a CI Linux box).
fn default_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(pf) = std::env::var("ProgramFiles") {
        roots.push(PathBuf::from(pf).join("NVIDIA GPU Computing Toolkit").join("CUDA"));
    }
    for (k, v) in std::env::vars() {
        if k == "CUDA_PATH" || k.starts_with("CUDA_PATH_V") {
            // CUDA_PATH points at a vX.Y dir; scan its parent so the version walk finds it.
            if let Some(parent) = PathBuf::from(&v).parent() {
                roots.push(parent.to_path_buf());
            }
        }
    }
    roots
}

/// Parse "v12.4" -> Version("12.4"); returns None for non-version dir names.
fn parse_version_dir(name: &str) -> Option<Version> {
    let stripped = name.strip_prefix('v').or_else(|| name.strip_prefix('V'))?;
    if !stripped.contains('.') {
        return None;
    }
    Version::parse(stripped).ok()
}

impl Installer for WindowsInstaller {
    fn scan(&self) -> Result<Vec<Candidate>> {
        let mut out = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for root in &self.roots {
            let entries = match std::fs::read_dir(root) {
                Ok(e) => e,
                Err(_) => continue, // root absent => nothing to adopt here
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let Some(version) = parse_version_dir(&name) else { continue };
                let dir = entry.path();
                if !dir.join("bin").join("nvcc.exe").exists() {
                    continue; // not a real toolkit dir
                }
                if seen.insert(dir.clone()) {
                    out.push(Candidate {
                        version,
                        root: dir,
                        platform: Self::windows_platform(),
                        source: Source::Adopted,
                    });
                }
            }
        }
        Ok(out)
    }

    fn adopt(&self, c: &Candidate) -> Result<Bundle> {
        let root = c.root.clone();
        anyhow::ensure!(
            root.join("bin").join("nvcc.exe").exists(),
            "adopt: {} is not a CUDA toolkit (no bin\\nvcc.exe)",
            root.display()
        );
        let toolkit = Toolkit {
            version: c.version.clone(),
            source: Source::Adopted,
            root,
            platform: c.platform.clone(),
            components: Vec::new(), // adopted: components unknown, not manifest-driven
            has_lib64: false,       // Windows uses lib\x64; lib64 symlink is Linux-only
            installed_at: OffsetDateTime::now_utc(),
            checksum: None,
        };
        Ok(Bundle { toolkit, cudnn: None, extra: Vec::new() })
    }

    fn acquire(&self, _plan: &AcquirePlan) -> Result<Vec<Cached>> {
        unimplemented!("windows acquire lands in WU-14")
    }
    fn verify(&self, _a: &[Cached]) -> Result<()> {
        unimplemented!("windows verify lands in WU-14")
    }
    fn extract_atomic(&self, _a: &[Cached], _tmp: &std::path::Path) -> Result<PathBuf> {
        unimplemented!("windows extract lands in WU-14")
    }
    fn place(&self, _tmp: &std::path::Path, _dst: &std::path::Path, _meta: &VersionMeta) -> Result<()> {
        unimplemented!("windows place lands in WU-14")
    }
    fn smoke_test(&self, _root: &std::path::Path) -> Result<()> {
        unimplemented!("windows smoke_test lands in WU-14")
    }
    fn ingest_supplied(&self, _file: &std::path::Path, _kind: ArtifactKind) -> Result<PathBuf> {
        unimplemented!("windows ingest lands in WU-14")
    }
}
```
   Note: `Candidate { version, root, platform, source }` and `Cached`/`AcquirePlan`/`ArtifactKind` come from `cuvm-app` (WU-1). If the WU-1 `Candidate` field names differ, adjust the struct-literal field names to match — types are owned by `cuvm-app`. The `Context` import is retained for the WU-14 follow-ups; if clippy flags it as unused here, drop it and re-add in WU-14.
4. - [ ] Step: Run, see pass.
```
cargo test -p cuvm-platform --test windows_adopt
```
Expected: `test result: ok. 1 passed`.
5. - [ ] Step: Add a negative test — a `v12.4` dir lacking `bin\nvcc.exe` is NOT adopted (guards against adopting an empty/partial dir). Append to `windows_adopt.rs`:
```rust
#[test]
fn scan_ignores_dir_without_nvcc() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child("CUDA/v12.4/extras/x.txt").write_str("").unwrap(); // no bin\nvcc.exe
    let installer = WindowsInstaller::with_roots(vec![tmp.child("CUDA").path().to_path_buf()]);
    assert!(installer.scan().unwrap().is_empty());
}
```
6. - [ ] Step: Run, see pass.
```
cargo test -p cuvm-platform --test windows_adopt
```
Expected: `test result: ok. 2 passed`.
7. - [ ] Step: Commit.
```bash
git add crates/cuvm-platform/src/windows/installer.rs crates/cuvm-platform/tests/windows_adopt.rs && git commit -m "feat(platform): windows scan/adopt of CUDA toolkit install trees

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 9.7 — Persistent user PATH: HKCU read-modify-write (no truncation, prepend not clobber)

**Files:**
- Create: `crates/cuvm-platform/src/windows/persist.rs`
- Test: `crates/cuvm-platform/tests/windows_persist.rs`

This is the syscall floor: the real registry write is `#[cfg(windows)]`. The **pure** part — computing the new PATH string from the old one (prepend the new bin, strip a prior cuvm bin, never truncate, never lose existing entries) — is host-neutral and gets full unit coverage on Linux. §2.2 mandates REG_EXPAND_SZ and forbids `setx` of a constructed PATH.

1. - [ ] Step: Write the failing test for the pure path-rewrite helper. It must (a) prepend the new bin, (b) remove a previously-injected cuvm bin (no duplicate), (c) preserve all unrelated entries verbatim, (d) never truncate at 1024 chars.
```rust
// crates/cuvm-platform/tests/windows_persist.rs
use cuvm_platform::windows::persist::compute_user_path;

#[test]
fn prepend_does_not_clobber_existing() {
    let old = r"C:\Windows;C:\Windows\System32;C:\Tools";
    let new_bin = r"C:\Users\dev\.cuvm\current\bin";
    let result = compute_user_path(old, new_bin, None);
    assert_eq!(
        result,
        r"C:\Users\dev\.cuvm\current\bin;C:\Windows;C:\Windows\System32;C:\Tools"
    );
}

#[test]
fn switching_default_strips_prior_cuvm_bin_no_dup() {
    let old = r"C:\Users\dev\.cuvm\current\bin;C:\Windows;C:\Tools";
    let new_bin = r"C:\Users\dev\.cuvm\current\bin"; // same junction path
    let prior = Some(r"C:\Users\dev\.cuvm\current\bin");
    let result = compute_user_path(old, new_bin, prior);
    assert_eq!(result, r"C:\Users\dev\.cuvm\current\bin;C:\Windows;C:\Tools");
    assert_eq!(result.matches(r".cuvm\current\bin").count(), 1, "no duplicate cuvm bin");
}

#[test]
fn long_path_is_not_truncated() {
    // > 1024 chars: proves we never go through setx's truncating path.
    let many: Vec<String> = (0..60).map(|i| format!(r"C:\Program Files\App{i}\bin")).collect();
    let old = many.join(";");
    assert!(old.len() > 1024);
    let new_bin = r"C:\Users\dev\.cuvm\current\bin";
    let result = compute_user_path(&old, new_bin, None);
    assert!(result.len() > old.len(), "result must contain the full old path plus new bin");
    assert!(result.ends_with(&old));
}
```
2. - [ ] Step: Run, see fail.
```
cargo test -p cuvm-platform --test windows_persist
```
Expected: fail — unresolved `cuvm_platform::windows::persist::compute_user_path`.
3. - [ ] Step: Implement `persist.rs`. The pure `compute_user_path` is always compiled; the actual HKCU write (`set_user_path`) is `#[cfg(windows)]` over the `windows` crate registry API with REG_EXPAND_SZ, plus a non-windows stub so the crate builds on Linux.
```rust
// crates/cuvm-platform/src/windows/persist.rs
use anyhow::Result;

/// Pure: build the new user PATH value. Prepend `new_bin`, drop any segment equal
/// to `prior_bin` or `new_bin` already present (idempotent, no duplicate), keep
/// every other segment verbatim and in order. NEVER truncates — caller must write
/// the whole string via the registry, never `setx` (1024-char truncation, §2.2).
pub fn compute_user_path(old: &str, new_bin: &str, prior_bin: Option<&str>) -> String {
    let mut segments: Vec<&str> = old
        .split(';')
        .filter(|s| !s.is_empty())
        .filter(|s| *s != new_bin && Some(*s) != prior_bin)
        .collect();
    let mut out = Vec::with_capacity(segments.len() + 1);
    out.push(new_bin);
    out.append(&mut segments);
    out.join(";")
}

#[cfg(windows)]
mod sys {
    use super::*;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, WPARAM};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    fn to_utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn read_user_path() -> Result<String> {
        unsafe {
            let mut hkey = HKEY::default();
            RegOpenKeyExW(HKEY_CURRENT_USER, w!("Environment"), 0, KEY_READ, &mut hkey).ok()?;
            let name = to_utf16("Path");
            let mut size: u32 = 0;
            // First call sizes the buffer.
            let _ = RegQueryValueExW(hkey, PCWSTR(name.as_ptr()), None, None, None, Some(&mut size));
            let mut buf = vec![0u8; size as usize];
            let r = RegQueryValueExW(
                hkey,
                PCWSTR(name.as_ptr()),
                None,
                None,
                Some(buf.as_mut_ptr()),
                Some(&mut size),
            );
            RegCloseKey(hkey).ok()?;
            if r.is_err() {
                return Ok(String::new()); // no user Path yet
            }
            let wide: &[u16] =
                std::slice::from_raw_parts(buf.as_ptr() as *const u16, (size as usize) / 2);
            let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
            Ok(String::from_utf16_lossy(&wide[..end]))
        }
    }

    pub fn set_user_path(new_bin: &str, prior_bin: Option<&str>) -> Result<()> {
        let old = read_user_path()?;
        let next = super::compute_user_path(&old, new_bin, prior_bin);
        unsafe {
            let mut hkey = HKEY::default();
            RegOpenKeyExW(HKEY_CURRENT_USER, w!("Environment"), 0, KEY_WRITE, &mut hkey).ok()?;
            let data = to_utf16(&next);
            let bytes = std::slice::from_raw_parts(
                data.as_ptr() as *const u8,
                data.len() * std::mem::size_of::<u16>(),
            );
            let r = RegSetValueExW(hkey, w!("Path"), 0, REG_EXPAND_SZ, Some(bytes));
            RegCloseKey(hkey).ok()?;
            r.ok()?;
            // Broadcast so already-open shells/Explorer pick up the change.
            let env = to_utf16("Environment");
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(env.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            );
        }
        Ok(())
    }
    // Silence the unused-import lint for HANDLE/HWND on builds that don't reference them.
    #[allow(unused_imports)]
    use {HANDLE as _Handle, HWND as _Hwnd};
}

#[cfg(not(windows))]
mod sys {
    use super::*;
    /// Non-windows stub so the crate compiles on the gnu/linux host.
    pub fn set_user_path(_new_bin: &str, _prior_bin: Option<&str>) -> Result<()> {
        anyhow::bail!("set_user_path is only available on windows")
    }
}

pub use sys::set_user_path;
```
4. - [ ] Step: Run the pure-logic tests, see pass.
```
cargo test -p cuvm-platform --test windows_persist
```
Expected: `test result: ok. 3 passed`.
5. - [ ] Step: Confirm the crate still builds clean on the linux host (the `#[cfg(not(windows))]` stub path).
```
cargo build -p cuvm-platform
```
Expected: `Finished` with no errors (the `windows` crate is not compiled on this target).
6. - [ ] Step: Commit.
```bash
git add crates/cuvm-platform/src/windows/persist.rs crates/cuvm-platform/tests/windows_persist.rs && git commit -m "feat(platform): HKCU user PATH r-m-w (REG_EXPAND_SZ, no truncation) + broadcast

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 9.8 — `mklink /J` junction current-pointer (create + repoint)

**Files:**
- Create: `crates/cuvm-platform/src/windows/junction.rs`
- Test: `crates/cuvm-platform/tests/windows_persist.rs` (extend)

The junction is the Windows analogue of the Unix `current` symlink. Per §2.2 it must be a `mklink /J` **directory junction** (no admin) — not a `/D` symlink. The cross-platform-testable contract is the create/repoint state machine (a fresh junction is created; repointing first removes the old one); the win32 `DeviceIoControl` reparse-point write is `#[cfg(windows)]`. On the linux host we exercise the same `set_junction` entrypoint, which falls back to a real directory symlink for test purposes via `#[cfg(unix)]`, proving the create-then-repoint logic.

1. - [ ] Step: Write the failing test. On the test host, `set_junction` creates `current` pointing at `v12.4`, then repointing at `v12.6` makes `current` resolve to the new target (no leftover/old link, no error on existing link).
```rust
// append to crates/cuvm-platform/tests/windows_persist.rs
use assert_fs::prelude::*;
use cuvm_platform::windows::junction::set_junction;

#[test]
fn junction_create_then_repoint() {
    let tmp = assert_fs::TempDir::new().unwrap();
    tmp.child("versions/12.4/bin").create_dir_all().unwrap();
    tmp.child("versions/12.6/bin").create_dir_all().unwrap();
    let link = tmp.child("current");

    // create
    set_junction(link.path(), &tmp.child("versions/12.4").path()).unwrap();
    assert!(link.path().join("bin").exists());

    // repoint (must succeed over an existing link, no manual cleanup)
    set_junction(link.path(), &tmp.child("versions/12.6").path()).unwrap();
    let resolved = std::fs::canonicalize(link.path()).unwrap();
    assert!(resolved.ends_with("12.6"), "junction must now point at 12.6, got {resolved:?}");
}
```
2. - [ ] Step: Run, see fail.
```
cargo test -p cuvm-platform --test windows_persist junction_create_then_repoint
```
Expected: fail — unresolved `cuvm_platform::windows::junction::set_junction`.
3. - [ ] Step: Implement `junction.rs`. `set_junction` first removes any existing link at the path, then creates a new one: a true directory junction via the `windows` crate on Windows, and a directory symlink on Unix (so the create/repoint logic is testable on the linux host).
```rust
// crates/cuvm-platform/src/windows/junction.rs
use std::path::Path;

use anyhow::Result;

/// Create or re-point a directory junction (Windows) / dir symlink (test host)
/// at `link` pointing to `target`. Removes any existing link first so re-pointing
/// the cuvm "current" pointer is idempotent (§2.2).
pub fn set_junction(link: &Path, target: &Path) -> Result<()> {
    remove_existing(link)?;
    create_dir_link(link, target)
}

fn remove_existing(link: &Path) -> Result<()> {
    // symlink_metadata so we inspect the link itself, never follow it.
    match std::fs::symlink_metadata(link) {
        Ok(_) => {
            // A junction/dir-link is removed as a directory entry on Windows;
            // on Unix a dir symlink is removed via remove_file.
            #[cfg(windows)]
            {
                std::fs::remove_dir(link).or_else(|_| std::fs::remove_file(link))?;
            }
            #[cfg(not(windows))]
            {
                std::fs::remove_file(link).or_else(|_| std::fs::remove_dir_all(link))?;
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(windows)]
fn create_dir_link(link: &Path, target: &Path) -> Result<()> {
    // A directory junction needs no admin (unlike a /D symlink). We shell out to
    // the built-in `mklink /J` to stay within documented, admin-free behavior (§2.2).
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .status()?;
    anyhow::ensure!(status.success(), "mklink /J failed for {}", link.display());
    Ok(())
}

#[cfg(not(windows))]
fn create_dir_link(link: &Path, target: &Path) -> Result<()> {
    // Test-host stub: a real directory symlink reproduces the create/repoint
    // semantics so the state machine is covered on linux CI.
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}
```
4. - [ ] Step: Run the junction test, see pass.
```
cargo test -p cuvm-platform --test windows_persist junction_create_then_repoint
```
Expected: `test result: ok. 1 passed`.
5. - [ ] Step: Run the full persist suite to confirm path-rewrite + junction all green together.
```
cargo test -p cuvm-platform --test windows_persist
```
Expected: `test result: ok. 4 passed`.
6. - [ ] Step: Commit.
```bash
git add crates/cuvm-platform/src/windows/junction.rs crates/cuvm-platform/tests/windows_persist.rs && git commit -m "feat(platform): mklink /J junction current-pointer create + repoint

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 9.9 — `cuvm.ps1` + `cuvm.cmd` shims (embedded, golden)

**Files:**
- Create: `shims/cuvm.ps1`
- Create: `shims/cuvm.cmd`
- Modify: `crates/cuvm-cli/src/shims.rs`
- Test: `crates/cuvm-cli/tests/shim_windows.rs`

The CLI embeds the shims via `include_str!` (per §3 / contract). The PowerShell module function `Invoke-Expression`s the env script for the activation subcommands and passes through everything else; the cmd shim writes a temp `.bat`, `CALL`s it, then `DEL`s it (§8). The golden test asserts the embedded content matches the on-disk shim and that the protocol invariants are present.

1. - [ ] Step: Create `shims/cuvm.ps1` (mirrors §8 exactly; activation verbs are `use|env|shell|default`).
```powershell
# cuvm.ps1 — PowerShell shim. Dot-source from $PROFILE: . "$HOME\.cuvm\shims\cuvm.ps1"
function cuvm {
    if ($args.Count -gt 0 -and ($args[0] -in 'use','env','shell','default')) {
        (& cuvm.exe @args --shell powershell | Out-String) | Invoke-Expression
    } else {
        & cuvm.exe @args
    }
}
```
2. - [ ] Step: Create `shims/cuvm.cmd` (temp `.bat` + `CALL` + `DEL`, per §8).
```bat
@echo off
:: cuvm.cmd — cmd.exe shim (degraded: no cd-hook). Put this dir on PATH.
set "_CUVM_VERB=%~1"
if /I "%_CUVM_VERB%"=="use"     goto :emit
if /I "%_CUVM_VERB%"=="env"     goto :emit
if /I "%_CUVM_VERB%"=="shell"   goto :emit
if /I "%_CUVM_VERB%"=="default" goto :emit
cuvm.exe %*
goto :eof
:emit
set "_CUVM_TMP=%TEMP%\cuvm-%RANDOM%.bat"
cuvm.exe env %* --shell cmd --out "%_CUVM_TMP%" && call "%_CUVM_TMP%" && del "%_CUVM_TMP%"
set "_CUVM_TMP="
set "_CUVM_VERB="
```
3. - [ ] Step: Write the failing test asserting `cuvm-cli` exposes the embedded Windows shims and they carry the protocol invariants. Assumes WU-6 established `cuvm_cli::shims` with `unix_bash()`/`unix_zsh()`; this adds `windows_powershell()` and `windows_cmd()`.
```rust
// crates/cuvm-cli/tests/shim_windows.rs
use cuvm_cli::shims;

#[test]
fn powershell_shim_evals_activation_verbs_and_passes_through() {
    let s = shims::windows_powershell();
    // Activation verbs go through Invoke-Expression of the printed env script.
    assert!(s.contains("Invoke-Expression"));
    assert!(s.contains("--shell powershell"));
    assert!(s.contains("'use','env','shell','default'"));
    // Non-activation commands pass straight through.
    assert!(s.contains("& cuvm.exe @args"));
}

#[test]
fn cmd_shim_uses_temp_bat_call_del() {
    let s = shims::windows_cmd();
    assert!(s.contains("--shell cmd --out"));
    assert!(s.to_lowercase().contains("call "));
    assert!(s.to_lowercase().contains("del "));
    assert!(s.contains("%TEMP%"));
}
```
4. - [ ] Step: Run, see fail.
```
cargo test -p cuvm-cli --test shim_windows
```
Expected: fail — `cannot find function 'windows_powershell' in module 'shims'`.
5. - [ ] Step: Add the embedding accessors to `crates/cuvm-cli/src/shims.rs`.
```rust
// add to crates/cuvm-cli/src/shims.rs (alongside the existing unix accessors)

/// PowerShell module function (dot-sourced into $PROFILE).
pub fn windows_powershell() -> &'static str {
    include_str!("../../../shims/cuvm.ps1")
}

/// cmd.exe shim (degraded shell: manual `cuvm use` only, no cd-hook).
pub fn windows_cmd() -> &'static str {
    include_str!("../../../shims/cuvm.cmd")
}
```
6. - [ ] Step: Run, see pass.
```
cargo test -p cuvm-cli --test shim_windows
```
Expected: `test result: ok. 2 passed`.
7. - [ ] Step: Commit.
```bash
git add shims/cuvm.ps1 shims/cuvm.cmd crates/cuvm-cli/src/shims.rs crates/cuvm-cli/tests/shim_windows.rs && git commit -m "feat(cli): embed powershell + cmd shims (Invoke-Expression / temp-bat CALL)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

#### Task 9.10 — End-to-end `cuvm env --shell powershell` + `hook --shell powershell` via the binary (assert_cmd)

**Files:**
- Test: `crates/cuvm-cli/tests/shim_windows.rs` (extend)

This proves the composition root wires `new_activator(Os::Windows)` into the hidden `env`/`hook` plumbing subcommands (§7) so the shims above actually receive a script. The binary is invoked on the linux host (emission is runtime-dispatched), with `--os windows` forcing the Windows backend regardless of host. Assumes the `env`/`hook` subcommands + an `--os` override flag were added in WU-8; if WU-8 used a different override mechanism (e.g. `CUVM_FORCE_OS` env), substitute it here.

1. - [ ] Step: Write the failing e2e test. `cuvm env 12.4 --shell powershell --os windows` must print a PowerShell script setting `$env:CUDA_PATH`; `cuvm hook --shell powershell --os windows` must print the chained `prompt` function.
```rust
use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn env_powershell_emits_cuda_path() {
    Command::cargo_bin("cuvm")
        .unwrap()
        .args(["env", "12.4", "--shell", "powershell", "--os", "windows"])
        .assert()
        .success()
        .stdout(contains("$env:CUDA_PATH"))
        .stdout(contains("$env:CUVM_INJECTED"))
        .stdout(contains("-split ';'"));
}

#[test]
fn hook_powershell_emits_chained_prompt() {
    Command::cargo_bin("cuvm")
        .unwrap()
        .args(["hook", "--shell", "powershell", "--os", "windows"])
        .assert()
        .success()
        .stdout(contains("function global:prompt"))
        .stdout(contains("__cuvm_prev_prompt"));
}
```
2. - [ ] Step: Run, see fail (until the `--os` override + Windows wiring are in place). If WU-8 already wired Windows emission, the env test may pass and only the assertions guide refinement; if not, expect a non-zero exit / missing flag.
```
cargo test -p cuvm-cli --test shim_windows env_powershell_emits_cuda_path hook_powershell_emits_chained_prompt
```
Expected: fail — either `unexpected argument '--os'` or the env subcommand not dispatching the Windows activator.
3. - [ ] Step: Minimal wiring in `cuvm-cli`. Add (if absent) an `--os <linux|windows>` global override that defaults to the host OS, and in the `env`/`hook` handlers select the activator via `cuvm_platform::new_activator(os)`.
```rust
// in the composition root (e.g. crates/cuvm-cli/src/main.rs handlers)
use cuvm_core::Os;
use cuvm_platform::new_activator;

fn resolve_os(flag: Option<&str>) -> Os {
    match flag {
        Some("windows") => Os::Windows,
        Some("linux") => Os::Linux,
        _ if cfg!(windows) => Os::Windows,
        _ => Os::Linux,
    }
}

// env handler:
fn cmd_env(spec: &str, shell: cuvm_core::Shell, os_flag: Option<&str>) -> anyhow::Result<()> {
    let os = resolve_os(os_flag);
    let activator = new_activator(os);
    let bundle = /* resolver.resolve(spec)?.bundle  (WU-2/WU-8 wiring) */
        resolve_bundle(spec)?;
    print!("{}", activator.emit_env(&bundle, shell)?);
    Ok(())
}

// hook handler:
fn cmd_hook(shell: cuvm_core::Shell, os_flag: Option<&str>) -> anyhow::Result<()> {
    let activator = new_activator(resolve_os(os_flag));
    print!("{}", activator.hook(shell)?);
    Ok(())
}
```
   Note: `resolve_bundle` is WU-8's existing resolver wiring; this task only adds the `os` selection + Windows dispatch around it. Add the `--os` field to the `env`/`hook` clap subcommand structs.
4. - [ ] Step: Run, see pass.
```
cargo test -p cuvm-cli --test shim_windows env_powershell_emits_cuda_path hook_powershell_emits_chained_prompt
```
Expected: `test result: ok. 2 passed`.
5. - [ ] Step: Run the full WU-9 surface (platform + cli) to confirm no regressions across emission, persist, adopt, shims.
```
cargo test -p cuvm-platform && cargo test -p cuvm-cli --test shim_windows
```
Expected: all green — platform golden (7) + adopt (2) + persist (4) + cli shim (4).
6. - [ ] Step: Final compile-everywhere check (the gate that the crate builds on the gnu/linux host with all `#[cfg(windows)]` floors stubbed).
```
cargo build --workspace
```
Expected: `Finished` with no errors and no `windows`-crate compilation on this target.
7. - [ ] Step: Commit.
```bash
git add crates/cuvm-cli && git commit -m "feat(cli): wire windows activator into env/hook with --os override (e2e)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

**WU-9 done when:** `cargo build --workspace` is clean on the gnu/linux host (Windows syscall floor stubbed); the PowerShell + cmd `emit_env`/`emit_deactivate`/`hook` golden snapshots are committed and stable; `compute_user_path` proves prepend-not-clobber + no truncation; `set_junction` proves create-then-repoint; `scan`/`adopt` adopt a fixture `vX.Y` tree as `Source::Adopted`; the embedded ps1/cmd shims carry the print-then-eval protocol; and `cuvm env/hook --os windows` emit the correct scripts via the runtime-dispatched activator. The win32-syscall bodies (`persist::set_user_path`, `junction` real junction) run on the **windows CI lane**; everything pure runs on the linux unit/shell lanes. Compat-data gate honored: no Windows toolkit fixture is ≥ 13.0 (Windows = N/A from 13.0).

---

