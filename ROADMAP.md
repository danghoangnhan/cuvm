# cuvm Roadmap & Timeline

`cuvm` is an [nvm](https://github.com/nvm-sh/nvm)-style version manager for the CUDA
toolkit (and, later, cuDNN), built in Rust for Linux/WSL **and** Windows with no root
and zero runtime dependencies. Development proceeds in milestones; each milestone is a
self-contained, shippable increment delivered as a set of work units (WUs) behind a
green CI gate.

| Milestone | Scope | Work units | Status | Tracking |
|---|---|---|---|---|
| **M1 — Switch core** | `adopt` / `use` / `current` / `which` / `default` / `alias` / `pin` + `.cuda-version` cd-hooks + `doctor` v1 (driver-ceiling + PATH lint), cross-platform (Linux/WSL + Windows), **no downloading** | WU-0 – WU-9 | ✅ **Shipped** — `v0.1.0` (2026-06-09) | [Milestone #1](https://github.com/danghoangnhan/cuvm/milestone/1) · [PR #1](https://github.com/danghoangnhan/cuvm/pull/1) · [Release v0.1.0](https://github.com/danghoangnhan/cuvm/releases/tag/v0.1.0) |
| **M2 — Install / download** | `ls-remote` / `install` / `uninstall` from NVIDIA per-component redistributables → `~/.cuvm/versions/<ver>` (Linux + Windows), driver-ceiling gate, atomic place + `lib64→lib` symlink + compile/link smoke test | WU-10 – WU-15 | ✅ **Shipped** — [PR #3](https://github.com/danghoangnhan/cuvm/pull/3) | [Milestone #2](https://github.com/danghoangnhan/cuvm/milestone/2) |
| **M3 — cuDNN bundling** | Pair + install a compatible cuDNN per toolkit (full `libcudnn*` set), EULA-gated auto-download + user-supplied ingestion, content-addressed store, `doctor` v2 pairing validation | WU-16 – WU-18 | ✅ **Shipped** — [PR #4](https://github.com/danghoangnhan/cuvm/pull/4) | [Milestone #3](https://github.com/danghoangnhan/cuvm/milestone/3) |
| **M4 — Companion libs + polish** | NCCL + cuBLAS-extra slots, `exec` / `shell`, shell completions, richer `ls-remote`, integration/smoke harness | WU-19 – WU-21 | ✅ **Shipped** — `exec`/`shell` + completions + richer `ls-remote` (WU-21, [#5](https://github.com/danghoangnhan/cuvm/pull/5)); NCCL companion (WU-20, [#6](https://github.com/danghoangnhan/cuvm/pull/6)/[#8](https://github.com/danghoangnhan/cuvm/pull/8)); cuBLAS-extra slot — `install --with <math-libs>` (WU-20c, [#11](https://github.com/danghoangnhan/cuvm/pull/11)); cross-cutting integration harness (WU-19, [#10](https://github.com/danghoangnhan/cuvm/pull/10)) | [Milestone #4](https://github.com/danghoangnhan/cuvm/milestone/4) |

## Timeline

```
2026-06-08  Spec approved (verified foundation: redist relocatability, corrected compat tables)
2026-06-09  ▼ M1 shipped  — v0.1.0 — adopt/switch/pin/doctor, Linux/WSL + Windows, no download
2026-06-09  ▼ M2 shipped  — install/download (NVIDIA redist toolkits)
2026-06-11  ▼ M3 shipped  — cuDNN bundling (pair / EULA gate / content store / doctor v2)
2026-06-14  ▼ M4 shipped  — companion libs (NCCL + cuBLAS-extra) + exec/shell + completions + integration harness
```

*Target dates are indicative and tracked via the GitHub [Milestones](https://github.com/danghoangnhan/cuvm/milestones).*

## Principles

- **Adopt, never destroy** — existing system installs and the GPU driver are respected (read-only); `uninstall` only de-registers adopted toolkits (ADR-005).
- **No root / no admin** for the common path; per-shell activation by default, opt-in persistent default.
- **Cross-platform parity** — one core, per-OS activation/installer backends behind traits; identical command surface.
- **Zero runtime dependencies** — a single static binary; pure-Rust archive handling keeps the musl build fully static.
- **Safe by default** — atomic installs (never-partial), mandatory sha256 verification, driver-compatibility checks that warn rather than silently break.
