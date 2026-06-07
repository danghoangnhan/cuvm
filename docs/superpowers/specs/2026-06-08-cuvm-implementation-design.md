# cuvm — Implementation Design Spec

*Status: Approved for implementation · Date: 2026-06-08 · Owner: Daniel Du*

This spec turns the prior design package (`CVM-Design-Document.md`, `CVM-ADRs.md`,
`CUDA-Version-Manager-Research.md`) into a code-level, buildable implementation plan.
It supersedes those documents where they conflict (notably the **name** and the
**implementation language**), and it folds in the results of a verification workflow
(`cuvm-design-research`, 2026-06-08) that empirically tested the load-bearing
assumptions and corrected one factual error in the draft compatibility data.

> **A future implementer should treat §2 (Verified Foundation) as ground truth** —
> those facts were tested against authoritative NVIDIA sources (and, for Linux
> relocatability, proven by actually compiling + linking CUDA programs from a
> relocated prefix). Do not re-derive them from memory.

---

## 1. Locked decisions

| Axis | Decision | Source |
|---|---|---|
| **Name / command / home** | `cuvm` · command `cuvm` · `~/.cuvm` (Unix), `%USERPROFILE%\.cuvm` (Windows) · module `github.com/danghoangnhan/cuvm` | user, 2026-06-08 (resolves design doc open-Q #4: "cvm" collides on GitHub) |
| **Language** | **Rust** (1.92.0 present) — supersedes ADR-001's Go decision | user, 2026-06-08 |
| **Scope** | **Full M1–M4** (switch → install → cuDNN bundling → companion libs) | user |
| **Platforms** | **Cross-platform day one** — Linux/WSL + Windows | user |
| **Execution** | **Fully autonomous**, **milestone order** with parallel Linux/Windows tracks, ship-candidate checkpoints | user |
| Switching scope | Per-shell env mutation default; opt-in persistent global default | ADR-002 |
| Switchable unit | **Bundle** = toolkit + pinned cuDNN | ADR-003 |
| Cross-platform strategy | One core, per-OS Activator/Installer backends; identical command surface | ADR-004 |
| Coexistence | Adopt existing installs in place; never manage the GPU driver | ADR-005 |
| cuDNN acquisition | Auto-download from NVIDIA redist (account-free) gated by a first-fetch EULA acknowledgement; user-supplied always available | ADR-006 (refined) |

The existing repo *directory* stays `cvm` and the remote stays
`github.com/danghoangnhan/cvm` unless renamed; all **code, command, home dir, and
breadcrumb vars use `cuvm`/`CUVM_*`**. WU-0 sweeps `cvm`→`cuvm` in the doc set.

---

## 2. Verified foundation (ground truth — tested 2026-06-08)

### 2.1 Linux toolkit acquisition — RELOCATABILITY CONFIRMED (proven by compile+link)
- Acquire downloaded toolkits from the **per-component redistributable tarballs**
  driven by `redistrib_<ver>.json` at
  `https://developer.download.nvidia.com/compute/cuda/redist/`. **Do not use the
  `.run` runfile** — it demands root for a driver step even with
  `--toolkitpath`/`--defaultroot`. Keep the runfile only as an offline fallback.
- `nvcc` self-locates every component via `bin/nvcc.profile`'s `$(_HERE_)`
  (`TOP = $(_HERE_)/..`), so the tree works from **any** prefix with no baked-in
  absolute paths. The shim only needs to set `CUDA_HOME`, prepend `bin` to PATH,
  and prepend `lib64` to `LD_LIBRARY_PATH`.
- **MANDATORY `lib64 → lib` symlink.** Redist tarballs ship `lib/`, but
  `nvcc.profile` links `-L$(TOP)/lib64`. Without the symlink, linking fails with
  `cannot find -lcudart`. This is the single highest-likelihood implementation bug.
  (Adopted `/usr/local/cuda-X.Y` installs natively use `lib64/` and need no fix.)
- **Component set is version-dependent and names change — discover from the
  manifest, never hardcode.** CUDA **12.x**: `cuda_nvcc` + `cuda_cudart` suffice
  (nvcc bundles `cicc`/libdevice/`crt/host_config.h`). CUDA **13.x**: nvcc was
  unbundled — you also need `cuda_crt`, `cccl`, `libnvvm`. Component *versions* are
  independent within a release (13.3.0 → `cuda_nvcc`=13.3.33, `cuda_cudart`=13.3.29,
  `cccl`=13.3.3.3.1). The CCCL key is `cuda_cccl` in 13.0/13.1/13.2 and **renamed to
  `cccl` only at 13.3** — so resolve component keys/paths dynamically from the
  manifest. Always **verify sha256** from the manifest (matched exactly in testing).
- Recommended usable component set: `cuda_nvcc`, `cuda_cudart`, `cuda_crt`, `cccl`,
  `libnvvm`, `cuda_nvrtc` (+ math libs `libcublas`, `libcufft`, `libcurand`,
  `libcusolver`, `libcusparse`, `libnpp`, `libnvjitlink` on request).
- **Export `CUDA_HOME` + `CUDA_PATH` + `CUDAToolkit_ROOT`** (CMake `FindCUDAToolkit`
  reads the latter two, not `CUDA_HOME`).
- **`nvcc` needs an external host `gcc/g++`** (no redist ships one); an incompatible
  host gcc breaks compilation. The **post-install compile+link smoke test** must
  surface this (with the `--allow-unsupported-compiler`/`-ccbin` hint), not hide it.
- Verified for **linux-x86_64 only**; `linux-sbsa`/`aarch64` share the structure but
  are unvalidated — gate arm64 download-install behind its own integration run.

### 2.2 Windows acquisition — ACQUIRE-CAPABLE, NO ADMIN
- Primary path: download the **windows-x86_64 redist `.zip`** components (exist
  through 13.3), verify sha256, merge into `%USERPROFILE%\.cuvm\versions\vX.Y`. No
  admin, no registry, no MSI.
- The official `.exe` is a 7-zip self-extractor (extractable without admin) but its
  silent `-s` install requires admin (writes `C:\Program Files` + HKLM + driver) —
  so it is a documented secondary fallback only (sets no env).
- **Per-shell activation:** process-scoped `$env:CUDA_PATH` + PATH prepend (no admin).
- **Persistent default (no admin):** read-modify-write **user** PATH in
  `HKCU\Environment` (REG_EXPAND_SZ; never `setx` a constructed PATH — 1024-char
  truncation) + broadcast `WM_SETTINGCHANGE` ("Environment") via `SendMessageTimeout`
  + a **`mklink /J` junction** `…\.cuvm\current → versions\vX.Y` (junctions need no
  admin; symlinks `/D` would). Switching the default = re-point the junction.
- Adopt existing installs via `CUDA_PATH`/`CUDA_PATH_VX_Y` and
  `C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\vX.Y` (read-only).
- Replicate `CUDA_PATH` + `bin` as the hard contract; `CUDA_PATH_VX_Y` and `libnvvp`
  are under-documented installer behaviors → best-effort. Auto-degrade a version to
  **adopt-only** if enterprise lockdown (AppLocker/WDAC/proxy/SmartScreen) blocks the
  download.

### 2.3 cuDNN — ACCOUNT-FREE REDIST, INSTALL-AND-USE GRANT
- `https://developer.download.nvidia.com/compute/cudnn/redist/` is a public,
  account-free index with `redistrib_X.Y.Z.json` (per-product, per-platform,
  per-`cuda_variant`, with sha256/md5/size). License is per-product (`cudnn` →
  `cudnn/LICENSE.txt`), not top-level.
- cuvm relies on the **install-and-use grant** (downloading NVIDIA binaries from
  NVIDIA hosts to the user's own machine), **not** the redistribution grant. Auto-
  download may be the default, **gated behind an explicit first-fetch EULA
  acknowledgement** (acceptance-by-use → cuvm implements the acceptance moment).
  Never download silently. User-supplied ingestion always available (air-gapped).
- **Link the full `libcudnn*` set** — cuDNN 9 is a loader that `dlopen`s engine
  sub-libs (`libcudnn_engines_*`, `_ops`, `_cnn`, `_adv`, `_graph`, `_heuristic`);
  linking only `libcudnn.so` breaks at runtime. Copy/symlink the libs + headers into
  the active toolkit dir (already on PATH) so switching needs no extra path entries.
- **Select cuDNN by CUDA major** for the dynamic-link path (cuda12 build works across
  all 12.x; cuda13 across all 13.x). Do **not** generalize to static linking.
- Store cuDNN **content-addressed by sha256** under `~/.cuvm/cudnn/<sha256>/`.
- **NCCL** secondary: its redist mirror (`compute/redist/nccl/`) has **no JSON
  manifest and no checksums** — self-record checksums; treat as opt-in.

### 2.4 Compatibility data — DRAFT REFUTED, CORRECTED
Authoritative source: **Table 3 "CUDA Toolkit and Corresponding Driver Versions"** in
the CUDA Toolkit Release Notes (not the cuda-compatibility landing page). Encode as
**data** with separate Linux/Windows columns; **compare versions as parsed integer
tuples, never lexically**. Snapshot = CUDA 13.3, mid-2026; ship a refresh/override
path.

- **CORRECTION (load-bearing):** the draft said Windows went N/A "at CUDA 13.1." It
  is **CUDA 13.0** — the Windows display driver was unbundled at 13.0, so **all of
  13.x has Windows = N/A**. (This must be a regression test.)
- **No CUDA 12.7** (NVIDIA skipped it).
- See §12 for the full encoded tables.
- **cuDNN ↔ CUDA matrix:** `8.9.7 → [11,12]` (last 8.x), `9.23.0 → [12,13]` (dropped
  11.x). Rule: **CUDA 13.x requires cuDNN 9.x; CUDA 11.x requires cuDNN 8.x.**
- **Forward-compat (`cuda-compat`)** raises the ceiling only on **data-center / NGC-
  ready RTX / Jetson** GPUs (never GeForce), Linux only, with base-branch minimums
  r450/r525/r580 for cuda-compat-11/12/13. Gate any suggestion behind a GPU-class
  check.
- **Minor-version-compatibility floors** ("likely works"): 525.60.13 for all 12.x,
  580.65.06 for all 13.x. Use strict per-release minimums for a "guaranteed" verdict.

### 2.5 Shim protocol — print-then-eval, breadcrumb cleanup
- The binary prints shell code to **stdout** (diagnostics to **stderr**); the shim
  `eval`s (bash/zsh) / `Invoke-Expression`s (pwsh) / writes-temp-`.bat`-and-`CALL`s
  (cmd). cmd has **no reliable cd-hook** (degraded shell; manual `cuvm use` only).
- **`CUVM_INJECTED` breadcrumb** records exactly the segments cuvm prepended, so the
  next switch strips precisely those before prepending (more robust than nvm's PATH
  regex). **Never strip `/usr/lib/wsl/lib`** (WSL driver libs).
- cd auto-activation: bash `PROMPT_COMMAND` (catches pushd/popd), zsh
  `add-zsh-hook chpwd`, powershell `prompt()` override that **chains** any existing
  prompt (oh-my-posh/Starship). Revert to the `default` alias when leaving a pinned
  dir (mirrors nvm `load-nvmrc`). `.cuda-version` discovered by **upward walk**.
- See §9 for the exact emitted scripts.

---

## 3. Architecture

Clean architecture realized as a **Cargo workspace of library crates** so the
Dependency Rule is enforced by the compiler (a crate cannot use what it does not
depend on; cycles are rejected). One core, per-OS backends behind traits (ADR-004).

```
cmd/cli ──▶ app (use-cases + trait ports) ──▶ core (pure domain)
                         ▲                         ▲
                         └── platform, store, registry, download, nvidia (leaf adapters)
```

- **`cuvm-core`** has **zero I/O dependencies** (no async runtime, no http, no fs in
  its public API) — pure logic + embedded data.
- **`cuvm-app`** declares the trait ports it needs; leaf crates implement them; the
  binary (`cuvm-cli`) is the **composition root** that wires concrete impls.
- **Backend dispatch is runtime** for *script emission* (so both backends compile on
  every host and Windows golden tests run on Linux CI); `#[cfg(windows)]`/
  `#[cfg(unix)]` is confined to the **syscall floor** (registry, junction, broadcast,
  symlink).

### 3.1 Workspace layout

```
cuvm/
├── Cargo.toml                # [workspace] members + shared dep versions
├── rust-toolchain.toml       # pin stable; targets for cross builds
├── crates/
│   ├── cuvm-core/            # Version (int-tuple), Bundle, Alias, Pin, Manifest,
│   │   └── data/             #   EnvPlan, compat decisions; embedded compat JSON (include_str!)
│   ├── cuvm-app/             # use-cases + TRAIT PORTS; depends only on cuvm-core
│   ├── cuvm-platform/        # Activator + Installer impls; unix (#[cfg(unix)]) + windows
│   ├── cuvm-store/           # atomic manifest/.cuvm-meta I/O + content-addressed cudnn store
│   ├── cuvm-registry/        # parse redistrib_<ver>.json (serde flatten)
│   ├── cuvm-download/        # ureq+rustls, sha256, tar.xz/zip extract (zip-slip guard)
│   ├── cuvm-nvidia/          # nvidia-smi probe (graceful-absent)
│   └── cuvm-cli/             # BINARY: clap command tree + composition root; include_str! shims
├── shims/                    # cuvm.sh, cuvm.zsh equiv, cuvm.ps1, cuvm.cmd, hook.* (embedded by cli)
├── tests/                    # workspace-level integration tests
└── testdata/                 # golden manifests, nvidia-smi captures, golden env scripts
```

### 3.2 Crate dependency rule (compiler-enforced)
`cuvm-core` → (nothing internal). `cuvm-app` → `cuvm-core` only. Leaf crates
(`platform`, `store`, `registry`, `download`, `nvidia`) → `cuvm-core` (+ each other
only where a typed view genuinely wraps another, e.g. `registry` → `download`).
`cuvm-cli` → everything (the only crate allowed to know concrete types).

### 3.3 Key crates (lean, for a small static binary + fast cd-hook startup)
- CLI **`clap`** (derive). Errors **`thiserror`** (core) + **`anyhow`** (app/cli edge).
- HTTP **`ureq`** + **`rustls`** — blocking, no async runtime; parallel downloads via
  `std::thread`. Resumable via `Range`.
- **`serde`/`serde_json`** (manifests; redist's "extra keys are components" handled by
  `#[serde(flatten)] HashMap`). **`sha2`** (hashing).
- Archives **`tar`** + **`lzma-rs`** (pure-Rust xz *decode* → keeps musl build fully
  static, no C `liblzma`), **`zip`** (Windows). Custom zip-slip guard on both.
- Windows **`windows`** crate (registry/junction/broadcast). Embedding **`include_str!`**.
- Tests **`insta`** (golden env scripts), **`assert_cmd`**+**`predicates`** (CLI e2e),
  **`mockall`** (trait mocks), **`tempfile`**/**`assert_fs`** (fs fixtures),
  **`httpmock`** (downloader).
- Static + cross: target `x86_64-unknown-linux-musl`; **`cargo-zigbuild`** to cross to
  linux arm64 + windows from one box.

---

## 4. Domain model & core types (Rust sketches)

```rust
// cuvm-core::version — ALWAYS compare numerically (drivers 3-part, cccl 4+-part)
pub struct Version { pub fields: Vec<u32>, pub raw: String }
impl Version { pub fn parse(s: &str) -> Result<Self>; pub fn major(&self) -> u32; }
// Ord/PartialOrd compare field-by-field, missing tail = 0.

pub enum Os { Linux, Windows }            // macOS shim works; CUDA unsupported there
pub enum Arch { X86_64, Sbsa, Aarch64 }   // mirror redist platform dirs
pub struct Platform { pub os: Os, pub arch: Arch }  // redist_key() -> "linux-x86_64"

pub enum Shell { Bash, Zsh, PowerShell, Cmd }
pub enum Source { Adopted, Downloaded, Supplied }   // ADR-005

pub struct Toolkit {
    pub version: Version, pub source: Source, pub root: PathBuf, pub platform: Platform,
    pub components: Vec<String>,   // from the manifest, never hardcoded
    pub has_lib64: bool,           // false => downloaded redist; lib64->lib symlink required
    pub installed_at: OffsetDateTime, pub checksum: Option<String>,
}
pub struct Cudnn {
    pub version: Version, pub cuda_major: u32, pub source: Source,
    pub store: PathBuf, pub sha256: String, pub libs: Vec<String>,  // full libcudnn* set
}
pub struct Bundle { pub toolkit: Toolkit, pub cudnn: Option<Cudnn>, pub extra: Vec<Companion> }
pub struct Alias { pub name: String, pub target: String }   // re-resolved by Resolver
pub struct Pin { pub spec: String, pub file: PathBuf }       // .cuda-version (upward walk)
pub struct Driver { pub present: bool, pub version: Version, pub platform: Platform,
                    pub gpu_class: GpuClass }                // GeForce => no cuda-compat
pub enum GpuClass { Unknown, GeForce, DataCenter, Jetson, NgcReadyRtx }
```

Manifest / sidecar (serde): `Manifest { schema_version, bundles[], aliases{}, pins{},
last_driver? }`; `BundleRecord { version, source, path, cudnn?, components?, sha256?,
installed_at }`; per-version `~/.cuvm/versions/<ver>/.cuvm-meta.json` = `VersionMeta`
adding `has_lib64`.

---

## 5. Trait ports (declared in `cuvm-app`)

```rust
pub trait Resolver {                                   // spec + cwd -> installed Bundle
    fn resolve(&self, spec: &str) -> Result<Resolved>; // exact/minor/major/latest/alias
    fn resolve_from_dir(&self, cwd: &Path) -> Result<Option<Resolved>>; // .cuda-version, else default
    fn expand_alias(&self, name: &str) -> Result<String>;
    fn find_pin_upward(&self, cwd: &Path) -> Result<Option<Pin>>;
}
pub trait Activator {                                  // per-OS; prints, shim evals
    fn emit_env(&self, b: &Bundle, sh: Shell) -> Result<String>;   // strip CUVM_INJECTED, set
    fn emit_deactivate(&self, sh: Shell) -> Result<String>;        //   CUDA_HOME/PATH/Toolkit_ROOT,
    fn hook(&self, sh: Shell) -> Result<String>;                   //   prepend bin/lib64, rewrite breadcrumb
    fn supports(&self, sh: Shell) -> bool;
}
pub trait Installer {                                  // per-OS; never-partial (temp + atomic rename)
    fn acquire(&self, plan: &AcquirePlan) -> Result<Vec<Cached>>;
    fn verify(&self, arts: &[Cached]) -> Result<()>;  // mandatory sha256
    fn extract_atomic(&self, arts: &[Cached], tmp: &Path) -> Result<PathBuf>; // zip-slip guard, strip wrapper
    fn place(&self, tmp: &Path, dst: &Path, meta: &VersionMeta) -> Result<()>; // lib64->lib, meta, rename
    fn smoke_test(&self, root: &Path) -> Result<()>;  // nvcc compile+link; catches lib64 + host-gcc
    fn ingest_supplied(&self, file: &Path, kind: ArtifactKind) -> Result<PathBuf>;
}
pub trait Inventory {                                  // scan/adopt + manifest; never deletes adopted
    fn list(&self) -> Result<Vec<Bundle>>;
    fn scan(&self) -> Result<Vec<Candidate>>;         // /usr/local/cuda-*, C:\...\CUDA\v*, CUDA_PATH
    fn adopt(&self, c: &Candidate, cudnn: Option<&Cudnn>) -> Result<Bundle>;
    fn deregister(&self, handle: &str) -> Result<()>; // adopted => de-register only (ADR-005)
    fn set_alias(&self, name: &str, target: &str) -> Result<()>;
    fn load(&self) -> Result<Manifest>; fn save(&self, m: &Manifest) -> Result<()>; // atomic
}
pub trait RegistryClient {                             // manifest-driven; NEVER constructs filenames
    fn list_toolkits(&self, p: &Platform) -> Result<Vec<Version>>;
    fn list_cudnn(&self, p: &Platform, cuda_major: u32) -> Result<Vec<Version>>;
    fn resolve_toolkit(&self, v: &Version, p: &Platform, want: &ComponentPolicy) -> Result<Vec<Artifact>>;
    fn resolve_cudnn(&self, v: &Version, p: &Platform, cuda_major: u32) -> Result<Vec<Artifact>>;
}
pub trait DriverProbe { fn probe(&self) -> Result<Driver>; }  // nvidia-smi, read-only, graceful-absent
pub trait CompatEngine {                              // tables as DATA (§12)
    fn max_toolkit_for_driver(&self, d: &Driver) -> Result<Version>;  // inverse lookup
    fn check_toolkit(&self, d: &Driver, want: &Version, strict: bool) -> Verdict; // warn+--force, not block
    fn pair_cudnn(&self, toolkit: &Version, available: &[Version]) -> Option<Version>; // by CUDA major
    fn validate_pair(&self, toolkit: &Version, cudnn: &Version) -> Verdict;
}
```

`EnvPlan` (OS-neutral, in core) is the intermediate the Activator renders per shell —
makes golden-file tests trivial. `Artifact { component, relative_path, url, sha256,
md5, size }` mirrors one redist platform object (`relative_path` taken **verbatim**).

---

## 6. State & directory layout

```
$CUVM_HOME (~/.cuvm | %USERPROFILE%\.cuvm)
├── manifest.json            # source of truth: bundles, aliases, pins, lastDriver
├── shims/                   # extracted shim scripts (stable path to source); re-extracted on version drift
├── versions/
│   └── <ver>/               # downloaded: merged redist tree (+ lib64->lib symlink) + .cuvm-meta.json
│   └── current → <ver>      # opt-in default pointer (symlink Unix / junction Windows)
├── cudnn/<sha256>/          # content-addressed cuDNN payloads, linked into version dirs
├── cache/                   # resumable, checksum-verified downloads
└── eula/                    # recorded NVIDIA EULA acknowledgements (first-fetch gate)
```
Adopted installs are **referenced in place** (heterogeneous paths in the manifest);
`uninstall` de-registers them without deleting files.

---

## 7. Command surface (unchanged from design doc §6, name = `cuvm`)

Inspection: `ls`, `ls-remote [--cudnn]`, `current`, `which <spec>`, `doctor`.
Lifecycle: `install <ver> [--cudnn <ver>|--no-cudnn] [--force]`, `adopt [--scan]`,
`uninstall <ver>`, `cudnn install <ver|file> --for <toolkit>`, `cudnn ls`.
Activation: `use [<spec>]`, `exec <spec> -- <cmd>`, `shell <spec>`, `default <spec>`.
Project/aliases: `pin <spec>`, `alias <name> <ver>`, `unalias <name>`.
Plumbing (shim-only, hidden): `env <spec> --shell <s>`, `hook --shell <s>`.

Spec grammar: exact (`12.4.1`), minor (`12.4` → newest patch), major (`12` → newest
`12.x`), `latest`, alias names, `.cuda-version` contents.

---

## 8. Switching mechanism — env-script contract

`cuvm env <spec> --shell <s>` prints **only** env code to stdout. Shims (installed
once into rc):

```sh
# bash/zsh
cuvm() { case "$1" in use|env|shell|default) eval "$(command cuvm "$@" --shell bash)";; *) command cuvm "$@";; esac; }
```
```powershell
function cuvm { if ($args[0] -in 'use','env','shell','default') { (& cuvm.exe @args --shell powershell | Out-String) | Invoke-Expression } else { & cuvm.exe @args } }
```
```bat
:: cuvm.cmd
@call cuvm.exe env %* --shell cmd --out "%TEMP%\cuvm-%RANDOM%.bat" && call "%TEMP%\cuvm-*.bat" && del "%TEMP%\cuvm-*.bat"
```

**bash** (`cuvm env 12.4 --shell bash`):
```sh
if [ -n "${CUVM_INJECTED:-}" ]; then
  PATH="$(printf '%s' "$PATH" | awk -v RS=: -v ORS=: -v inj="$CUVM_INJECTED" \
    'BEGIN{n=split(inj,a,":");for(i=1;i<=n;i++)d[a[i]]=1} !($0 in d)&&NF{print}' | sed 's/:$//')"
  # same strip on LD_LIBRARY_PATH; NEVER strips /usr/lib/wsl/lib
fi
export CUDA_HOME="$HOME/.cuvm/versions/12.4.1"
export CUDA_PATH="$CUDA_HOME"; export CUDAToolkit_ROOT="$CUDA_HOME"
export PATH="$CUDA_HOME/bin:$PATH"
export LD_LIBRARY_PATH="$CUDA_HOME/lib64:${LD_LIBRARY_PATH:-}"
export CUVM_CURRENT="12.4.1"
export CUVM_INJECTED="$CUDA_HOME/bin:$CUDA_HOME/lib64"
```
**powershell** strips `CUVM_INJECTED` from `$env:Path`, sets
`$env:CUDA_PATH/CUDA_HOME/CUDAToolkit_ROOT`, prepends `…\bin`, rewrites breadcrumb.
**Windows default** (`cuvm default`): re-point `…\.cuvm\current` junction (mklink /J),
R-M-W user PATH in HKCU + `WM_SETTINGCHANGE` broadcast. `hook` emits PROMPT_COMMAND
(bash) / chpwd (zsh) / chained `prompt()` (pwsh); cmd warns it is unsupported.

---

## 9. Install pipeline

```
cuvm install 12.4
  ├─ Resolver: 12.4 -> newest patch in registry for OS/arch
  ├─ Compat gate: driver ceiling check (warn + --force + cuda-compat hint; not hard-block)
  ├─ Registry: fetch redistrib_<ver>.json; pick component set DYNAMICALLY from manifest keys
  ├─ Download: components -> cache/ (resumable); MANDATORY sha256 verify
  ├─ Extract: tar.xz/zip into versions/.tmp-<ver>/ (zip-slip guard, strip wrapper dir)
  ├─ Linux: create lib64 -> lib symlink (MANDATORY)
  ├─ (cuDNN by default: install paired cuDNN into the tmp tree — §10)
  ├─ Atomic rename .tmp-<ver> -> versions/<ver>; write .cuvm-meta.json + manifest
  └─ SMOKE TEST: compile+link a tiny cudart program from the prefix (catches lib64 + host-gcc)
```
Windows: merge windows-x86_64 redist `.zip` components; auto-degrade to adopt-only if
download blocked. Never-partial via temp + atomic rename.

---

## 10. cuDNN bundling

`install` pairs a compatible cuDNN by default (CUDA major → cuDNN line via §12
matrix); `--no-cudnn` opts out, `--cudnn <ver>` overrides. First auto-download prompts
**EULA acknowledgement** (recorded under `~/.cuvm/eula/`). cuDNN extracted to the
content-addressed `cudnn/<sha256>/` store, then the **full `libcudnn*` set + headers**
are linked/copied into the active toolkit dir (symlink Unix, junction/copy Windows) so
`use` needs no extra path entries. `cuvm cudnn install <file> --for <tk>` ingests user-
supplied archives (zip-slip guarded, content-addressed) — the always-available path.

---

## 11. Compatibility & doctor

`doctor` reports: driver→toolkit ceiling (`nvidia-smi` → §12 inverse lookup; warns if
the active toolkit exceeds it, explains `cuda-compat` only for eligible GPU classes);
cuDNN pairing vs the matrix; PATH/`LD_LIBRARY_PATH` hygiene (stale/dup CUDA entries,
`nvcc` not matching `CUDA_HOME`); host-gcc compatibility; `nvcc --version` resolves to
the active bundle. Machine-readable exit codes (gates CI). Missing `nvidia-smi` →
"driver unknown, build-only OK" (no crash).

---

## 12. Compatibility data tables (encode as embedded JSON in `cuvm-core/data/`)

**Driver minimums per CUDA release** (Linux x86_64 / Windows x86_64; full dotted
strings; tuple compare). GA rows (add Update rows from Table 3 as needed):

| CUDA | Linux min | Windows min |
|---|---|---|
| 11.8 | 520.61.05 | 520.06 |
| 12.0 | 525.60.13 | 527.41 |
| 12.1 | 530.30.02 | 531.14 |
| 12.2 | 535.54.03 | 536.25 |
| 12.3 | 545.23.06 | 545.84 |
| 12.4 | 550.54.14 | 551.61 |
| 12.5 | 555.42.02 | 555.85 |
| 12.6 | 560.28.03 | 560.76 |
| 12.8 | 570.26 | 570.65 |
| 12.9 | 575.51.03 | 576.02 |
| 13.0 | 580.65.06 | **N/A** (driver unbundled at 13.0) |
| 13.1 | 590.44.01 | N/A |
| 13.2 | 595.45.04 | N/A |
| 13.3 | 610.43.02 | N/A |

(No CUDA 12.7.) **Inverse driver→ceiling** is derived at runtime: highest CUDA whose
per-OS minimum ≤ installed driver (e.g. Linux 552.x → 12.4, 565.x → 12.6).

**cuDNN ↔ CUDA:** `8.9.7 → [11,12]`, `9.23.0 → [12,13]`. CUDA 13.x **requires** cuDNN
9.x; CUDA 11.x **requires** cuDNN 8.x.

**Minor-version-compat floors** ("likely works"): 525.60.13 (all 12.x), 580.65.06
(all 13.x). **cuda-compat** base branches: r450/r525/r580 (11/12/13), data-center/NGC-
RTX/Jetson only, Linux only.

---

## 13. Testing strategy

- **Unit (pure, fast, no GPU/net):** Resolver spec grammar + ordering (incl. 4-part
  `cccl` versions, alias cycle rejection, find-up to root); Compat tables (inverse
  lookup, tuple compare `570.26` < `570.124.06`, **Windows-N/A-from-13.0 regression**,
  cuDNN-major rules); manifest round-trip; redist parse with **dynamic component
  keys** (`cuda_cccl` 13.0–13.2 vs `cccl` 13.3) + 404-guard.
- **Golden (`insta`):** emitted env scripts per shell (bash/zsh/powershell/cmd);
  repeated-`use` no PATH duplication; WSL `/usr/lib/wsl/lib` preserved; `hook` output.
- **Security:** zip-slip guard (crafted `../../evil` + absolute + symlink-escape
  entries rejected, nothing written outside dst) for tar and zip.
- **Integration (containers, pre-staged tiny/mirrored toolkits — no multi-GB pulls):**
  adopt→default→use→pin→cd-switch→doctor (both OSes); Linux install + **nvcc
  compile+link smoke** (proves lib64 + manifest-driven set + host-gcc); cuDNN link +
  pairing doctor. Windows lane assembles from staged `.zip` fixtures on a **non-admin
  runner** (proves the no-admin claim).
- **Doctor snapshot** on a deliberately broken PATH.

---

## 14. Autonomous build plan (WU-0 … WU-21)

TDD-first; each WU lists deliverable + the tests that define done + gating spike.
Front-loads zero-GPU/zero-network logic (~80% of suite). Naming = `cuvm`/`CUVM_*`/
`.cuda-version`.

**Foundations:** **WU-0** workspace + clap skeleton + naming sweep + CI cross-compile.
**WU-1** trait ports + runtime factory + backend stubs (the seam enabling parallel
tracks).

**M1 (switch core, both OSes — ship-candidate):** **WU-2** Resolver + version grammar.
**WU-3** Manifest + Inventory (atomic I/O). **WU-4** Linux adopt (`/usr/local/cuda-*`).
**WU-5** Linux Activator + `CUVM_INJECTED` cleanup (golden). **WU-6** Unix shims +
`hook` (black-box `bash --norc`/`zsh -f`). **WU-7** Compat engine + embedded tables
(incl. Windows-13.0 regression). **WU-8** `use/current/which/default/alias/pin/ls` +
`doctor` v1. **WU-9** (parallel) Windows backend: ps1/cmd emitters (golden), HKCU R-M-W
+ `WM_SETTINGCHANGE` + `/J` junction, adopt, shim + chained-prompt hook.

**M2 (install):** **WU-10** redist registry/parser (`ls-remote`; dynamic keys test).
**WU-11** resumable checksum-verified downloader. **WU-12** extractor + zip-slip guard.
**WU-13** Linux assembler (merge + **lib64 fix** + version-branched set + atomic +
smoke). **WU-14** Windows assembler (redist `.zip`) **or** auto adopt-only fallback.
**WU-15** `install`/`uninstall` wiring + driver-ceiling gate.

**M3 (cuDNN):** **WU-16** cuDNN redist client + pairing (full `libcudnn*` set). **WU-17**
`cudnn install` auto-download + EULA gate + user-supplied ingestion + content-addressed
store + link-into-toolkit. **WU-18** `doctor` v2 (pairing validation).

**M4 (polish):** **WU-19** integration harness + smoke framework (start in M2). **WU-20**
NCCL (self-recorded checksums) + cuBLAS-extra slot. **WU-21** `exec`/`shell`,
completions, richer `ls-remote`.

**Dependency graph (critical path bold):**
```
WU-0 ▶ WU-1 ┬▶ WU-2 ┬▶ **WU-8** ▶ (M1)
            ├▶ WU-3 ┤
            ├▶ WU-7 ┘        ┌▶ WU-10 ▶ WU-13 ┐
            ├▶ WU-5 ▶ WU-6 ──┤  WU-11 ▶ WU-14 ├▶ **WU-15** ▶ (M2)
            └▶ WU-9 (Win ∥)  └▶ WU-12 ────────┘
WU-15 ▶ WU-16 ▶ WU-17 ▶ WU-18 ▶ (M3)   WU-19 (cross-cutting)   WU-20, WU-21 ▶ (M4)
```

**Spike→unit gating:** relocatability → WU-4/10–13/19; windows-install → WU-9/14;
cudnn → WU-16/17/20; compat (refuted→corrected) → WU-7/9/15/18; shim → WU-5/6/9.

**Autonomy strategy:** WU-0→1 serial; then Linux track (2,3,5,6,7,8) ∥ Windows track
(9) once WU-1's traits land on `main`. Worktree isolation per track + a dedicated
worktree for WU-7 data and WU-19 fixtures. **CI matrix:** compile-all (linux/amd64,
linux/arm64, windows/amd64) + unit/shell/windows/integration lanes; non-admin Windows
runner. **Checkpoints (commit at each green):** ① after WU-8/9 (M1), ② after WU-13
(Linux real-install smoke), ③ after WU-15 (install both OSes), ④ after WU-17/18 (cuDNN),
⑤ after WU-21.

**Fallbacks where full-M1–M4 + cross-platform realistically bends:** Windows per-
version adopt-only degrade (WU-14); arm64 deferred behind its own integration run
(x86_64 verified only); host-gcc surfaced not hidden (WU-13/doctor); cuDNN user-
supplied always available (WU-17); compat table is a dated snapshot with refresh path
(WU-7).

---

## 15. ADR updates

- **ADR-001 — SUPERSEDED (2026-06-08).** Original: Go core + shims. New: **Rust core +
  shims.** Rationale: the decisive axis is "native compiled static binary, not a
  script/interpreted runtime" (forces out bash/PowerShell on parity and Python on the
  fresh-VM runtime dependency + cd-hook cold start). Go vs Rust is a velocity/
  preference call with no decisive technical edge for this I/O-bound tool; owner chose
  Rust. The architecture is identical: traits replace interfaces, `include_str!`
  replaces `go:embed`, clap replaces cobra, `windows` crate replaces
  `golang.org/x/sys/windows`, `cargo-zigbuild`/targets replace `GOOS/GOARCH`.
- **ADR-002–005** unchanged. **ADR-006** refined: auto-download is permissible under
  the **install-and-use grant** (not redistribution) but **gated behind a first-fetch
  EULA acknowledgement**; user-supplied remains the always-available path.

---

## 16. Open items / risks

1. Confirm repo/module naming: code uses `cuvm`; repo dir + remote still `cvm`
   (rename or leave?).
2. Pin a small (~10–50 MB) locally-mirrored redist fixture subset so integration never
   pulls multi-GB toolkits.
3. `linux-sbsa`/`aarch64` relocatability + compile unvalidated (x86_64 only) — dedicated
   arm64 integration spike before committing arm64 download-install (arm64 can adopt
   day one).
4. Windows `CUDA_PATH_VX_Y` / `libnvvp` under-documented → `CUDA_PATH` + `bin` is the
   hard contract, the rest best-effort.
5. Concrete trigger spec for Windows auto-degrade-to-adopt-only (detect missing
   windows-x86_64 components / blocked downloads).
6. Compat table is a mid-2026 (CUDA 13.3) snapshot, partially refuted (Windows-N/A-
   from-13.0 baked as regression) — build the refresh/override path.
7. Host C/C++ compiler is required by `nvcc` and shipped by no redist — smoke test +
   doctor surface incompatibility with `--allow-unsupported-compiler`/`-ccbin` hints.

---

## Sources (verification workflow, 2026-06-08)
NVIDIA CUDA redist (`developer.download.nvidia.com/compute/cuda/redist/` +
`redistrib_<ver>.json`), cuDNN redist (`…/compute/cudnn/redist/` + LICENSE.txt + EULA),
CUDA Toolkit Release Notes Table 3, CUDA Compatibility (minor-version + forward-compat),
cuDNN Support Matrix, CUDA install guides (Linux + Windows); nvm (`nvm.sh`
`nvm_strip_path`/`nvm_change_path`/`nvm_find_nvmrc`), pyenv/rbenv `init -`, nvm-windows
(`nvm.go`), conda activation. Full URLs in the workflow transcript.
