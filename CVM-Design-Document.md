# CUVM — CUDA Version Manager: Design Document

*Status: Draft v1 · Date: 2026-06-08 · Owner: Daniel Tu*

A cross-platform CLI for installing, switching, pinning, and bundling multiple CUDA toolkits (and their companion libraries such as cuDNN), modeled on `nvm`/`gvm` but adapted to the realities of the CUDA stack.

> Companion documents: `CUDA-Version-Manager-Research.md` (prior-art survey) and `CVM-ADRs.md` (architecture decision records for the key trade-offs below).

---

## 1. Goals, non-goals, and scope

### 1.1 Goals
Provide a single tool that lets a developer **install, list, switch, pin-per-project, and remove multiple CUDA toolkits** on the same machine, on **both Windows and Linux/WSL**, without `sudo`/admin and without corrupting the system. Treat **cuDNN and other companion libraries as first-class, version-bundled components** so that "switch to CUDA 12.4" also lands a compatible cuDNN. Keep developers safe from the most common CUDA footgun by **checking the toolkit choice against the installed GPU driver**.

### 1.2 Non-goals (v1)
The tool does **not** install or manage the **GPU kernel driver** — that stays the OS/admin's responsibility; CUVM only reads it and reasons about compatibility. It does **not** manage Python framework wheels (PyTorch/TensorFlow ship their own bundled CUDA runtime — a separate axis CUVM stays out of). It is **not** a container tool; Docker/NGC images are an alternative, not a target.

### 1.3 Scope decision (from requirements)
v1 targets the **full** experience: discover/adopt → download/install → switch → pin → **cuDNN bundling** → uninstall, cross-platform from day one. Implementation is a **Go core binary + thin per-shell shims** (see ADR-001).

---

## 2. Requirements

### 2.1 Functional
The tool must discover toolkits already installed by NVIDIA's installers and adopt them without re-downloading; download and install new toolkits into a user-owned root; activate a chosen toolkit for the **current shell** (default) or persist a machine-default; resolve loose version specs (`12` → newest installed `12.x`, plus named aliases); read a per-project `.cuda-version` file and auto-activate on directory entry; install and pair a compatible **cuDNN** (and, stretch, NCCL/cuBLAS) per toolkit; report and diagnose the active environment (`doctor`); and uninstall cleanly.

### 2.2 Non-functional
- **No elevated privileges** for the common path (install into user space, switch via env vars). Adopting system installs is read-only.
- **Fast switching** — `use` must be sub-second (it only rewrites env vars; no I/O-heavy work).
- **Cross-platform parity** — same command surface and `.cuda-version` semantics on Windows and Linux/WSL; only the activation backend differs.
- **Safe by default** — never silently mutate global state; warn on driver-incompatible switches; atomic installs (no half-written version dirs visible as "installed").
- **Offline-friendly** — everything already installed works without network; only `install`/`ls-remote` need it.
- **Auditable state** — a single human-readable state/manifest file; deleting a version is deleting a directory.

### 2.3 Constraints (CUDA-specific, drives the whole design)
1. **Driver is system-level and backward-compatible only.** A newer driver runs older toolkits; the reverse needs the `cuda-compat` forward-compat package. Approx ceilings: driver 525→CUDA 12.0, 535→12.2, 545→12.3, 550→12.4. CUVM must know this and gate switches.
2. **cuDNN versions independently** and must match the toolkit's major (often minor) version.
3. **Install layouts differ by OS.** Linux: `/usr/local/cuda-X.Y` with a `/usr/local/cuda` symlink. Windows: `C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\vX.Y` with `CUDA_PATH`, `CUDA_PATH_VX_Y`, and `PATH` entries.
4. **Toolkits are large (2–4 GB)** and gated behind per-OS/per-distro installers; cuDNN historically required NVIDIA-account login.
5. **A child process cannot mutate its parent shell's environment** — activation must happen *in* the shell (sourced shim), not in the binary alone.

---

## 3. Domain model

```
Driver (system, not managed)  ──supports──▶  max CUDA toolkit version
        │ read via nvidia-smi
        ▼
Toolkit  (CUVM-managed unit)         e.g. 12.4.1
   ├── nvcc + compiler
   ├── runtime libs (cudart, cublas, …)
   ├── headers
   └── companion libs (CUVM-bundled, version-pinned):
         ├── cuDNN        e.g. 9.1 (for 12.x)
         ├── NCCL         (stretch)
         └── cuBLAS/extra (stretch)

Bundle = a Toolkit + its pinned companion libs, addressed by one version handle.
Alias  = a human name → bundle (default, ml, build, …).
Project pin = .cuda-version file → version spec, auto-activated on cd.
```

Key conceptual choice (see ADR-003): the **unit of switching is a *bundle*** (toolkit + pinned cuDNN), not a bare toolkit. `cuvm use 12.4` activates the toolkit *and* its associated cuDNN in one move. A bare-toolkit bundle (no cuDNN) is valid for users who only need `nvcc`.

---

## 4. High-level architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  User shell (bash / zsh / PowerShell / cmd)                        │
│   ┌────────────────────────────────────────────────────────────┐  │
│   │  Shell integration (sourced)                                 │  │
│   │   • defines `cuvm` function/alias                             │  │
│   │   • cd-hook reads .cuda-version                              │  │
│   │   • applies env mutations emitted by the core (eval)         │  │
│   └───────────────▲──────────────────────────┬─────────────────┘  │
└───────────────────┼──────────────────────────┼────────────────────┘
                    │ stdout: env script        │ argv
                    │                            ▼
        ┌───────────┴────────────────────────────────────────┐
        │  cuvm core  (single Go static binary)                 │
        │                                                      │
        │  ┌─────────────┐  ┌─────────────┐  ┌──────────────┐  │
        │  │ Resolver    │  │ Activator   │  │ Doctor /     │  │
        │  │ (spec→ver,  │  │ (emit env   │  │ Compat engine│  │
        │  │  aliases,   │  │  per OS)    │  │ (driver↔tk↔  │  │
        │  │ .cuda-ver)  │  │             │  │  cuDNN)      │  │
        │  └─────────────┘  └─────────────┘  └──────────────┘  │
        │  ┌─────────────┐  ┌─────────────┐  ┌──────────────┐  │
        │  │ Inventory   │  │ Installer   │  │ Registry     │  │
        │  │ (scan/adopt │  │ (download,  │  │ client       │  │
        │  │  + manifest)│  │  verify,    │  │ (ls-remote   │  │
        │  │             │  │  extract,   │  │  index)      │  │
        │  │             │  │  atomic)    │  │              │  │
        │  └─────────────┘  └─────────────┘  └──────────────┘  │
        └───────────────────────┬──────────────────────────────┘
                                │
                ┌───────────────┴───────────────┐
                ▼                                ▼
      ~/.cuvm  state + versions          OS package sources
      (manifest.json, versions/,        (NVIDIA toolkit archives,
       cudnn/, aliases/, cache/)         cuDNN archives, driver via
                                         nvidia-smi read-only)
```

**Why this shape.** The binary is the brain (download, extract, verify, resolve, compatibility logic, manifest); the shell shim is a dumb-but-essential mouth that `eval`s an environment script the binary prints. This is the proven `rbenv`/`nvm-windows` pattern and is the only way to satisfy both "fast, rich, cross-platform core logic" and "mutate the live shell" (ADR-001, ADR-002).

### 4.1 Components
- **Resolver** — turns a spec (`12`, `12.4`, `latest`, `default`, contents of `.cuda-version`) into a concrete installed bundle; owns alias expansion and project-file lookup (walks up from cwd like `.nvmrc`).
- **Activator** — given a resolved bundle, emits an OS-appropriate env script: Unix prints `export` lines (cleaning prior CUVM-injected entries first); Windows/PowerShell prints `$env:` assignments; `cmd` prints `set`. The shell shim `eval`s/invokes it.
- **Inventory** — scans known install locations to adopt existing toolkits, plus reads/writes the CUVM manifest of managed bundles.
- **Installer** — download → checksum verify → extract to a temp dir → atomically rename into `versions/<ver>` (so a failed install never appears installed). Handles cuDNN payloads the same way, slotting them into the toolkit dir.
- **Registry client** — fetches the list of installable toolkit/cuDNN versions and their URLs/checksums for the current OS+arch.
- **Doctor / Compat engine** — the CUDA-specific value-add: reads driver version via `nvidia-smi`, checks toolkit ≤ driver ceiling, checks cuDNN↔toolkit pairing, and lints `PATH`/`LD_LIBRARY_PATH` for stale or duplicate CUDA entries.

---

## 5. State & directory layout

```
$CUVM_HOME            (default ~/.cuvm on Unix, %USERPROFILE%\.cuvm on Windows)
├── manifest.json        # source of truth: managed bundles, adopted installs, aliases, pins
├── shims/               # cuvm.sh, cuvm.zsh, cuvm.ps1, cuvm.cmd  (shell integration)
├── versions/
│   ├── 11.8.0/          # mirrors NVIDIA layout
│   │   ├── bin/         (nvcc, cuda-gdb, …)
│   │   ├── lib64/  or  lib/x64/   (cudart, cublas, …)
│   │   ├── include/
│   │   ├── nvvm/
│   │   └── .cuvm-meta.json   # cuDNN paired, source (adopted|downloaded), checksum, install date
│   ├── 12.4.1/
│   └── 12.6.0/
├── cudnn/               # cuDNN payloads, content-addressed; linked/copied into version dirs
├── cache/               # downloaded installers/archives (resumable, checksum-verified)
└── aliases/             # default, ml, build → version handles
```

`manifest.json` (illustrative):
```json
{
  "schemaVersion": 1,
  "bundles": [
    {"version": "12.4.1", "source": "downloaded", "path": "versions/12.4.1",
     "cudnn": "9.1.0", "installedAt": "2026-06-08T10:00:00Z"},
    {"version": "11.8.0", "source": "adopted",
     "path": "/usr/local/cuda-11.8", "cudnn": null}
  ],
  "aliases": {"default": "12.4.1", "ml": "11.8.0"},
  "lastDriver": {"version": "550.90", "cudaCeiling": "12.4"}
}
```

Adopted installs are **referenced in place** (no copy) so CUVM coexists with NVIDIA's own installs.

---

## 6. Command surface

```
# Inspection
cuvm ls                      # installed bundles; marks active + default; shows paired cuDNN
cuvm ls-remote [--cudnn]     # toolkits (and cuDNN) available to install for this OS/arch
cuvm current                 # active bundle + driver + ceiling
cuvm which <spec>            # absolute path of a bundle
cuvm doctor                  # driver↔toolkit↔cuDNN checks + PATH lint

# Lifecycle
cuvm install <ver> [--cudnn <ver>|--no-cudnn]   # download+install toolkit (+cuDNN by default)
cuvm adopt [--scan]          # discover & register existing system installs
cuvm uninstall <ver>
cuvm cudnn install <ver> --for <toolkit>        # add/replace cuDNN in an existing bundle
cuvm cudnn ls

# Activation
cuvm use <spec>              # activate for THIS shell (export/$env mutation)
cuvm use                     # resolve from .cuda-version in cwd (walks upward)
cuvm exec <spec> -- <cmd>    # run one command under a version, no persistent switch
cuvm shell <spec>            # spawn a subshell with the version active
cuvm default <spec>          # persist machine/user default for new shells

# Project & aliases
cuvm pin <spec>              # write .cuda-version in cwd
cuvm alias <name> <ver>
cuvm unalias <name>

# Plumbing (used by shim, not typed by users)
cuvm env <spec> --shell <bash|zsh|powershell|cmd>   # print env script to eval
cuvm hook --shell <...>                              # print cd-hook integration
```

Version-spec grammar mirrors `nvm`: exact (`12.4.1`), minor (`12.4` → newest installed patch), major (`12` → newest installed `12.x`), `latest`, alias names, and the contents of `.cuda-version`.

---

## 7. The switching mechanism (deep dive, per OS)

The binary is invoked as `cuvm env <spec> --shell <s>` and prints a script; the shim runs it in-place. Both OSes first **strip any CUVM-injected entries** from the path-like vars (tracked via a `CUVM_INJECTED` breadcrumb var) so repeated `use` calls don't stack duplicate paths — this mirrors nvm's `nvm_change_path` cleanup.

### 7.1 Linux / WSL / macOS
Shim `cuvm.sh` is sourced from `~/.bashrc`/`~/.zshrc`. `cuvm use 12.4` results in:
```sh
export CUDA_HOME=$CUVM_HOME/versions/12.4.1
export CUDA_ROOT=$CUDA_HOME
export PATH=$CUDA_HOME/bin:<cleaned PATH>
export LD_LIBRARY_PATH=$CUDA_HOME/lib64:<cleaned LD_LIBRARY_PATH>
export CUVM_CURRENT=12.4.1
export CUVM_INJECTED=$CUDA_HOME           # breadcrumb for cleanup
```
The `cd`-hook (added via `cuvm hook`) checks for `.cuda-version` on each prompt/`cd` and re-activates as needed.

### 7.2 Windows (PowerShell primary, cmd supported)
No `source` concept; the shim is a PowerShell module function (and a `.cmd` wrapper). `cuvm use 12.4` (PowerShell):
```powershell
$env:CUDA_PATH = "$env:USERPROFILE\.cuvm\versions\12.4.1"
$env:CUDA_HOME = $env:CUDA_PATH
$env:Path = "$env:CUDA_PATH\bin;$env:CUDA_PATH\libnvvp;" + <cleaned Path>
$env:CUVM_CURRENT = "12.4.1"
$env:CUVM_INJECTED = $env:CUDA_PATH
```
For a **persistent default** (`cuvm default`), the core writes the *user* (not system, no admin) environment variables `CUDA_PATH`/`CUDA_PATH_V12_4` and updates a junction `…\.cuvm\versions\current → 12.4.1` so tools that hard-code a fixed path still resolve. cmd users get a `set`-based script and a doskey-style `cuvm` macro. A directory-change hook is provided via PowerShell's prompt function.

### 7.3 Why not just flip a symlink/junction globally?
That's the legacy `/usr/local/cuda` approach — global, often root, not per-shell or per-project. CUVM offers a junction *as an opt-in compatibility shim* for hard-coded toolchains, but the default and recommended mode is per-shell env mutation (ADR-002).

---

## 8. cuDNN & companion-library bundling

cuDNN is the headline differentiator. Design:

- Each bundle records a **paired cuDNN version** in `.cuvm-meta.json`. `cuvm install 12.4` installs a compatible cuDNN by default (resolved from a built-in compatibility table: cuDNN 9.x ↔ CUDA 12.x, cuDNN 8.x ↔ CUDA 11.x/12.x per matrix), `--no-cudnn` opts out, `--cudnn <ver>` overrides.
- cuDNN payloads live content-addressed under `cudnn/` and are **linked/copied into the toolkit's `lib`/`include`** so activation needs no extra path entries — switching the bundle switches cuDNN automatically.
- `cuvm doctor` validates the pairing against the matrix and flags mismatches (e.g. cuDNN 8.9 sitting in a CUDA 12.6 bundle that wants 9.x).
- **Licensing reality (ADR-006):** toolkits are freely downloadable; cuDNN historically requires an NVIDIA-account download. v1 supports **user-supplied cuDNN archives** (`cuvm cudnn install ./cudnn-….tar.xz --for 12.4`) and *attempts* direct download only where terms permit; otherwise it prints the exact URL and drops the user at the download page, then ingests the file. NCCL/cuBLAS-extra follow the same slot model as a stretch goal.

---

## 9. Download & install pipeline

```
cuvm install 12.4
  │
  ├─ Resolver: 12.4 → 12.4.1 (newest patch in registry for this OS/arch)
  ├─ Compat: driver ceiling check  ── fail→ warn + require --force / suggest cuda-compat
  ├─ Registry: fetch URL + checksum for OS/arch
  ├─ Installer:
  │     download → cache/ (resumable)
  │     verify checksum  ── mismatch→ abort, keep nothing
  │     extract to versions/.tmp-12.4.1/
  │     install paired cuDNN into the temp dir
  │     atomic rename .tmp-12.4.1 → versions/12.4.1
  │     write .cuvm-meta.json + update manifest.json
  └─ done (no partial state ever visible as "installed")
```
On Windows, where the official distributable is an MSI/exe rather than a relocatable archive, the installer either (a) drives a silent extraction into the version dir, or (b) for adopted MSI installs, references the standard `C:\Program Files\...` path. (See §16, open question 2.)

---

## 10. Compatibility & safety engine (`cuvm doctor`)

`doctor` is the trust-builder. It reports:
- **Driver vs active toolkit** — `nvidia-smi` driver version → CUDA ceiling; flags if the active/about-to-activate toolkit exceeds it, and explains the `cuda-compat` forward-compat option.
- **cuDNN pairing** — active toolkit ↔ bundled cuDNN against the built-in matrix.
- **Path hygiene** — duplicate/stale CUDA entries in `PATH`/`LD_LIBRARY_PATH`, `nvcc` on path not matching `CUDA_HOME`, leftover non-CUVM CUDA exports in shell rc files.
- **Sanity** — `nvcc --version` actually resolves to the active bundle.

Exit codes are machine-readable so `doctor` can gate CI.

---

## 11. Key flows (sequences)

**First-time setup**
```
install binary → `cuvm adopt --scan` (registers existing /usr/local/cuda-* or C:\…\CUDA\v*)
   → `cuvm default 12.4` → new shells start on 12.4
```

**Per-project**
```
cd project/  → cd-hook reads .cuda-version (=11.8) → bundle installed?
   yes → activate 11.8 (+ its cuDNN) for this shell
   no  → warn + offer `cuvm install 11.8`
```

**One-off build under another version**
```
cuvm exec 12.6 -- make    # 12.6 active only for that process; shell unchanged
```

---

## 12. Error handling & edge cases
Switching to an uninstalled version prompts an install rather than failing silently. Driver-incompatible switches warn and require `--force` (with a `cuda-compat` hint) rather than hard-blocking — the user may know better. Interrupted downloads resume from `cache/`; corrupted extracts never leave a visible version dir (atomic rename). Adopted system installs are never deleted by `cuvm uninstall` (only de-registered) to avoid destroying admin-managed state. Missing `nvidia-smi` (no NVIDIA GPU / driver) degrades gracefully: switching still works for build-only use, `doctor` notes the driver is unknown.

---

## 13. Security & privacy
No elevated privileges in the default path. All downloads are checksum-verified against the registry; HTTPS only; signatures verified where NVIDIA publishes them. CUVM never transmits telemetry. Writing the persistent Windows default touches **user** env scope only. The binary refuses to extract archives with path-traversal entries (zip-slip guard).

---

## 14. Testing strategy
Unit tests for the Resolver (spec grammar, alias/`.cuda-version` resolution, version ordering incl. patch selection) and the Compat engine (driver-ceiling table, cuDNN matrix). Golden-file tests for the env scripts emitted per shell (bash/zsh/powershell/cmd) so path-cleanup logic is locked. Integration tests in containers/VMs: a Linux image with two pre-staged toolkits to exercise adopt/use/switch without large downloads; a Windows runner for the PowerShell module and junction behavior. A `doctor` snapshot test on a known-broken PATH. (See the testing-strategy skill if you want a fuller plan.)

---

## 15. Roadmap (milestones)
- **M1 — Switch core, both OSes.** Binary + shims, `adopt`/`ls`/`use`/`current`/`which`/`default`/`alias`, `.cuda-version` + cd-hooks, `env`/`hook` plumbing, `doctor` (driver-ceiling + path lint). No downloading. *This is shippable and already beats every existing tool on pinning + safety.*
- **M2 — Install/download.** `ls-remote`, registry client, installer pipeline (atomic, resumable, verified) for toolkits.
- **M3 — cuDNN bundling.** Pairing table, `cudnn install`, user-supplied ingestion, doctor pairing checks. *The differentiator.*
- **M4 — Companion libs (NCCL, cuBLAS-extra) + polish.** Shell completions, `cuvm shell`, richer `ls-remote` filtering.

---

## 16. Open questions & risks
1. **Toolkit relocatability:** are NVIDIA's archive distributables fully relocatable into `$CUVM_HOME`, or do some components hard-code paths? Needs a spike on Linux runfile/`.tar` and the Windows installer before M2. *(Highest-risk unknown.)*
2. **Windows install format:** can we silently extract the official Windows toolkit into a user dir, or must we adopt MSI installs in place? Determines whether Windows gets full `install` or adopt-only in M2.
3. **cuDNN download terms:** confirm what can be auto-downloaded vs must be user-supplied; shapes M3 UX.
4. **Naming:** "cuvm" collides on GitHub (CMake VM, C/C++ VM, Composer VM, Cortex VM). Candidates: `cudaup` (rustup-style), `cudaenv`, `nvccvm`, `cuvm`.
5. **macOS:** modern CUDA doesn't support macOS; keep the Unix shim macOS-compatible for ergonomics but mark CUDA itself unsupported there.

---

*Decisions backing this design are recorded in `CVM-ADRs.md`.*
