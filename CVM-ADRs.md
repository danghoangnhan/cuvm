# CUVM — Architecture Decision Records

*Companion to `CVM-Design-Document.md`. Each ADR records one significant decision, the options weighed, and the consequences. Status reflects the locked requirements: cross-platform from day one, full scope including cuDNN, Word-of-the-author stack choice delegated to this analysis.*

---

# ADR-001: Implementation stack — Go core binary + thin shell shims

**Status:** Accepted
**Date:** 2026-06-08
**Deciders:** Daniel Tu (project owner)

## Context
CUVM must (a) mutate the *current* shell's environment, which a child process cannot do for its parent, and (b) do network downloads, checksum verification, archive extraction, JSON state, and OS-specific env handling — across **Windows and Linux/WSL equally**. The stack choice was delegated to this analysis given those constraints.

## Decision
Build a **single statically-linked Go binary** for all core logic, paired with **thin, sourced shell shims** (`cuvm.sh`/`.zsh`, `cuvm.ps1`, `cuvm.cmd`) that `eval` an environment script the binary prints. This is the `rbenv` / `nvm-windows` pattern.

## Options Considered

### Option A: Go core + shell shims (chosen)
| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium |
| Cross-platform | Excellent — one binary, GOOS cross-compile, no runtime dep |
| Distribution | Single file per OS/arch; trivial install |
| Shell mutation | Solved via printed env script + shim `eval` |

**Pros:** One codebase for all OSes; static binary needs no interpreter; excellent archive/HTTP/checksum libraries; fast startup keeps `use` sub-second; strong concurrency for downloads.
**Cons:** Still needs per-shell shim files; Go's Windows env/junction handling needs care.

### Option B: Rust core + shell shims
| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium-High |
| Cross-platform | Excellent |
| Distribution | Single binary |
| Shell mutation | Same shim approach |

**Pros:** Same architecture, very polished CLIs (clap), strong safety.
**Cons:** Higher upfront effort and build complexity; ecosystem for "drive an MSI / Windows env" slightly thinner than Go's; team familiarity likely lower. No decisive advantage over Go for this workload.

### Option C: Pure bash + PowerShell (nvm-style, no compiled core)
| Dimension | Assessment |
|-----------|------------|
| Complexity | Low (Unix) / Medium (split) |
| Cross-platform | Poor — two unrelated codebases |
| Distribution | Scripts only |
| Shell mutation | Native (sourced) |

**Pros:** Simplest possible Unix path; most nvm-like; zero build step.
**Cons:** Two divergent codebases violates the "cross-platform parity" requirement; bash is weak for robust downloads/checksums/atomic installs and JSON state; hard to test. Fails the full-scope + equal-platform mandate.

### Option D: Python
**Pros:** Fast to prototype, rich libraries.
**Cons:** Requires a Python runtime present (chicken-and-egg on fresh machines), slow cold start hurts the `cd`-hook path, and per-shell env mutation is still awkward (same shim trick needed anyway). Runtime dependency is disqualifying for a tool meant to be foundational.

## Trade-off Analysis
The decisive axes are **cross-platform parity** (kills C) and **no runtime dependency + fast startup** (kills D). Between Go and Rust the architecture is identical; Go wins on team velocity, build simplicity, and a marginally better Windows-automation story, with no material downside for this I/O-bound tool.

## Consequences
- Easier: cross-compiling one binary; testing core logic; robust downloads/atomic installs.
- Harder: must still author and maintain 3–4 small shim files; Go requires disciplined handling of Windows user-scope env vars and junctions.
- Revisit if: a hard requirement emerges that Go can't meet (none foreseen).

## Action Items
1. [ ] Scaffold Go module + cobra CLI with `env`/`hook` plumbing commands.
2. [ ] Author bash/zsh/powershell/cmd shims that `eval` the printed env script.
3. [ ] Set up GOOS/GOARCH cross-compile in CI for win/amd64 + linux/amd64 (+arm64).

---

# ADR-002: Switching scope — per-shell userland mutation (not global symlink)

**Status:** Accepted
**Date:** 2026-06-08
**Deciders:** Daniel Tu

## Context
"Switch CUDA version" can mean: change a global symlink/`CUDA_PATH` for the whole machine, or change only the calling shell. The first is the legacy Linux (`/usr/local/cuda`) and Windows-GUI approach; the second is the nvm philosophy. We require no-admin operation and per-project pinning.

## Decision
**Per-shell environment mutation is the default and primary mode.** A global junction/`current` pointer is offered only as an **opt-in compatibility shim** (`cuvm default`) for toolchains that hard-code a fixed path.

## Options Considered

### Option A: Per-shell env mutation (chosen)
**Pros:** No admin; per-project `.cuda-version` becomes possible; multiple versions active in different terminals simultaneously; reversible; matches nvm mental model.
**Cons:** Only affects shells with the shim loaded; GUI apps / IDEs launched outside the shell don't see it (mitigated by the opt-in default).

### Option B: Global symlink / `CUDA_PATH` flip
**Pros:** Affects everything system-wide including GUI/IDEs; one obvious state.
**Cons:** Needs root/admin; no per-project or per-shell isolation; race conditions across users; exactly the limitation that makes existing approaches painful.

## Trade-off Analysis
Per-shell is the only model compatible with no-admin + per-project pinning, which are core requirements. The legitimate need global mode serves — "an IDE launched from the GUI must find a specific CUDA" — is handled by the opt-in persistent default without making global the everyday mechanism.

## Consequences
- Easier: safe experimentation, project isolation, no privilege escalation.
- Harder: must educate users that GUI-launched processes need the persisted default; must implement robust prior-entry cleanup so repeated `use` doesn't stack PATH duplicates.
- Revisit if: telemetry shows most users only ever want one global version (then promote `default`).

## Action Items
1. [ ] Implement `CUVM_INJECTED` breadcrumb + path cleanup in the Activator.
2. [ ] Implement opt-in `cuvm default` writing user-scope env + `versions/current` junction/symlink.

---

# ADR-003: Unit of versioning — toolkit + cuDNN *bundle*, not bare toolkit

**Status:** Accepted
**Date:** 2026-06-08
**Deciders:** Daniel Tu

## Context
The full-scope mandate includes cuDNN. The question is whether cuDNN is a separate thing the user juggles, or part of the switchable unit. cuDNN must match the toolkit's major (often minor) version, and ML users almost always need both together.

## Decision
The **switchable unit is a *bundle*** = one toolkit + its pinned companion libs (cuDNN now; NCCL/cuBLAS-extra later). `cuvm use 12.4` activates the toolkit and its paired cuDNN atomically. A bundle with no cuDNN is valid for build-only users.

## Options Considered

### Option A: Bundle (toolkit + pinned cuDNN) — chosen
**Pros:** One command yields a coherent, compatible environment; impossible to half-switch into a cuDNN mismatch; matches how people actually use CUDA for ML.
**Cons:** More state per version; cuDNN coupling logic; storage duplication if many toolkits share a cuDNN (mitigated by content-addressed `cudnn/` store).

### Option B: Independent axes (toolkit and cuDNN switched separately)
**Pros:** Maximum flexibility; smaller core.
**Cons:** Pushes the hardest correctness problem (version matching) onto the user — the exact pain CUVM exists to remove; easy to end up with an incompatible pair.

## Trade-off Analysis
CUVM's reason to exist over `switch-cuda` is removing footguns. Letting users mismatch cuDNN re-introduces the footgun. Bundling, with a built-in compatibility matrix and `doctor` validation, is the differentiator. Flexibility is preserved via `--cudnn <ver>` / `--no-cudnn` overrides.

## Consequences
- Easier: "it just works" switching for ML; doctor can guarantee pairing validity.
- Harder: maintaining the cuDNN↔CUDA compatibility table; content-addressed storage to avoid bloat.
- Revisit: extend the bundle to NCCL/cuBLAS in M4.

## Action Items
1. [ ] Define `.cuvm-meta.json` bundle schema (paired cuDNN, source, checksum).
2. [ ] Build the cuDNN↔CUDA compatibility table as data, not code.
3. [ ] Content-address the `cudnn/` store; link into version dirs.

---

# ADR-004: Cross-platform strategy — one core, per-OS activation backends

**Status:** Accepted
**Date:** 2026-06-08
**Deciders:** Daniel Tu

## Context
Windows and Linux differ deeply: sourced shells + `LD_LIBRARY_PATH` vs PowerShell/`cmd` + `CUDA_PATH`/junctions; relocatable archives vs MSI installers. `nvm` solved this by maintaining **two entirely separate projects** (nvm and nvm-windows) with divergent commands — a real source of user confusion.

## Decision
**Single Go core with a platform-abstracted Activator/Installer interface**; only the backend implementations differ. The **command surface and `.cuda-version` semantics are identical** across OSes. We explicitly reject the nvm "two projects" split.

## Options Considered

### Option A: One core, pluggable per-OS backends (chosen)
**Pros:** Identical UX and docs everywhere; shared resolver/compat/manifest logic; one place for bugs.
**Cons:** Internal abstraction boundary to design carefully; Windows backend is genuinely more complex (env scope, junctions, MSI).

### Option B: Two codebases (nvm/nvm-windows style)
**Pros:** Each optimized for its OS; simpler individually.
**Cons:** Divergent commands confuse users; double maintenance; violates the cross-platform-parity requirement.

## Trade-off Analysis
The parity requirement is explicit, and the painful lesson of nvm-windows is that divergence hurts users for years. A clean `Activator`/`Installer` interface contains the per-OS complexity without forking the product.

## Consequences
- Easier: one mental model, one doc set, shared tests for OS-independent logic.
- Harder: must design the backend interface up front; Windows backend carries most of the risk (ties to §16 open questions 1–2 in the design doc).
- Revisit: if Windows install proves infeasible to relocate, Windows may be adopt-only for M2 while Linux gets full install (still one codebase).

## Action Items
1. [ ] Define `Activator` (emit env script) and `Installer` (acquire→place) interfaces.
2. [ ] Implement Linux backend first (M1), Windows backend in parallel where shared logic allows.

---

# ADR-005: Coexistence — adopt existing installs in place, never own the driver

**Status:** Accepted
**Date:** 2026-06-08
**Deciders:** Daniel Tu

## Context
Users (and CI/admins) already have CUDA installed via NVIDIA installers, and the GPU **driver** is a system/kernel component. CUVM must not fight or destroy these.

## Decision
CUVM **adopts** existing toolkit installs by reference (no copy, no move) and records them in the manifest as `source: adopted`. `cuvm uninstall` on an adopted install only **de-registers** it; it never deletes admin-managed files. CUVM **never installs, modifies, or removes the GPU driver** — it only reads it via `nvidia-smi` for compatibility reasoning.

## Options Considered

### Option A: Adopt-in-place + driver read-only (chosen)
**Pros:** Safe coexistence with NVIDIA installers and shared/HPC machines; no destruction of admin state; immediate value on day one (manage what's already there).
**Cons:** Adopted installs live outside `$CUVM_HOME` (heterogeneous paths in the manifest); can't guarantee their integrity.

### Option B: Manage-only (ignore/migrate existing installs)
**Pros:** Uniform `$CUVM_HOME` layout; full control.
**Cons:** Forces re-download of multi-GB toolkits already present; hostile on shared machines; slow first-run.

### Option C: Also manage the driver
**Cons:** Kernel-level, needs root, distro-specific, can brick a machine. Out of scope and dangerous.

## Trade-off Analysis
Adoption delivers the M1 "switch what you already have" value with zero downloads and is the respectful choice on shared systems. Touching the driver is categorically too risky and unnecessary given backward compatibility.

## Consequences
- Easier: instant utility; safe on managed machines.
- Harder: manifest must handle mixed in-place and CUVM-owned paths; `doctor` must reason about driver state it doesn't control.
- Revisit: never, for the driver boundary.

## Action Items
1. [ ] Implement `cuvm adopt --scan` for `/usr/local/cuda-*` and `C:\…\CUDA\v*`.
2. [ ] Guard `uninstall` to de-register (not delete) adopted installs.

---

# ADR-006: cuDNN acquisition — user-supplied first, auto-download where permitted

**Status:** Accepted
**Date:** 2026-06-08
**Deciders:** Daniel Tu

## Context
CUDA toolkits are freely downloadable; **cuDNN has historically required an NVIDIA-account/EULA-gated download**. We want cuDNN bundling (ADR-003) but cannot assume an unauthenticated download is always permitted.

## Decision
Support **user-supplied cuDNN archives** as the always-available path (`cuvm cudnn install <file> --for <toolkit>`), and **attempt direct download only where NVIDIA's terms and endpoints allow** (e.g. redistributable channels). When direct download isn't permitted, print the exact version-matched URL, send the user to obtain it, then ingest the downloaded file.

## Options Considered

### Option A: User-supplied first + opportunistic auto-download (chosen)
**Pros:** Always works regardless of licensing; respects EULA; auto-download is a bonus where allowed.
**Cons:** Extra manual step for users in the gated case.

### Option B: Always auto-download
**Pros:** Smoothest UX.
**Cons:** May violate EULA / break when auth changes; brittle.

### Option C: Never bundle cuDNN
**Cons:** Abandons the differentiator (ADR-003).

## Trade-off Analysis
Correctness and license-compliance outweigh a one-time manual fetch in the gated case. The ingestion + content-addressed store means a manually downloaded cuDNN is still managed, paired, and validated exactly like an auto-downloaded one.

## Consequences
- Easier: legal clarity; works in air-gapped/enterprise settings via supplied archives.
- Harder: must keep a per-version URL/checksum map current; UX must clearly guide the manual fetch.
- Revisit: if NVIDIA exposes a stable redistributable cuDNN channel, lean harder on auto-download.

## Action Items
1. [ ] Implement archive ingestion + checksum + zip-slip guard for cuDNN.
2. [ ] Maintain a cuDNN version→URL/checksum table; verify legality per channel before enabling auto-download.

---

## Decision summary

| ADR | Decision | One-line rationale |
|-----|----------|--------------------|
| 001 | Go core + shell shims | Only model meeting cross-platform parity, no runtime dep, and live-shell mutation |
| 002 | Per-shell mutation default | No-admin + per-project pinning; global is opt-in compat |
| 003 | Toolkit+cuDNN *bundle* | Removing version-mismatch footguns is the reason to exist |
| 004 | One core, per-OS backends | Identical UX everywhere; avoid nvm's two-project confusion |
| 005 | Adopt in place; never touch driver | Safe coexistence + instant value; driver is too dangerous to own |
| 006 | User-supplied cuDNN first | License-compliant and always works; auto-download where permitted |
