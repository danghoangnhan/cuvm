# Building a CUDA Version Manager (CVM): Prior Art & Design Research

*Research brief — June 2026. Goal: assess existing approaches to managing multiple CUDA versions, extract the design patterns that make `nvm`/`gvm` work, and propose an architecture for an `nvm`-style CUDA version manager CLI.*

---

## 1. Executive summary

There is **no widely-adopted, `nvm`-style version manager for CUDA**. The space is filled instead by (a) one-off bash scripts that switch environment variables, (b) symlink flipping, (c) Linux `environment-modules`, and (d) Conda/Mamba, which sidesteps the problem by installing per-environment runtimes. Each solves *switching* but none solves the full lifecycle — **discover → download → install → switch → pin-per-project → uninstall** — the way `nvm` does for Node.

That gap is real and worth filling, but CUDA is meaningfully harder than Node or Go for three reasons: the **GPU driver** is a separate, kernel-level component the tool can't freely swap; **cuDNN / cuBLAS / NCCL and other libraries** version independently of the toolkit; and the **Windows install model** (system-wide MSI installers, `CUDA_PATH`) resists the per-shell, userland approach that makes `nvm` clean on Unix. A successful tool has to embrace these constraints rather than hide them.

The recommended MVP: a tool that **switches between already-installed toolkits per-shell** (the safe, high-value 80%), with a `.cuda-version` project file, and only later layers in automated downloading. This matches how `nvm` actually earned trust.

---

## 2. Prior art — what already exists

### 2.1 Switching scripts (closest analog to nvm)

**`phohenecker/switch-cuda`** is the single closest thing to "nvm for CUDA." It's a sourced bash script: `source switch-cuda.sh 11.8` rewrites `PATH`, `LD_LIBRARY_PATH`, `CUDA_HOME`, and `CUDA_ROOT` for the current shell session; called with no argument it lists every CUDA install it finds under `/usr/local`. This is exactly the `nvm use` mechanic — modify the current shell, don't touch the system — but it stops there: no install, no download, no per-project pinning.

**`bycloudai/SwapCudaVersionWindows`** is a documented manual procedure (plus helper notes) for Windows: reorder `PATH` so the desired `...\CUDA\vXX.X\bin` sits first, and point `CUDA_PATH` at the chosen version. It's a guide, not a tool, and it edits *system* environment variables through the GUI.

### 2.2 Symlink flipping (the Linux default trick)

The canonical Linux approach is the `/usr/local/cuda` symlink. NVIDIA's installers create `/usr/local/cuda-11.8`, `/usr/local/cuda-12.4`, etc., and a `cuda` symlink pointing at the "active" one. Switching is `sudo ln -sfT /usr/local/cuda-12.4 /usr/local/cuda`. Simple and global, but it's **machine-wide, needs root, and isn't per-shell or per-project** — the opposite of nvm's philosophy.

### 2.3 Environment Modules (HPC heritage)

On shared/HPC Linux systems, `environment-modules` or `Lmod` is standard: `module load cuda/12.1` injects the right `PATH`/`LD_LIBRARY_PATH`, `module unload` reverses it, `module avail` lists options. This is genuinely close to what we want in spirit (clean load/unload, multiple coexisting versions) but requires modulefiles authored per install and is aimed at admins provisioning clusters, not individual developers self-serving on a laptop.

### 2.4 Conda / Mamba (the "just avoid the problem" route)

By far the most popular *practical* answer today. `conda install cudatoolkit=11.8` (and increasingly the `cuda-toolkit` metapackage plus `cudnn`) installs a CUDA **runtime** into the environment's prefix; activating the env puts the right libraries on the path. Per-project isolation comes free with the env. Limitations: historically it shipped the runtime libraries, **not the full toolkit/`nvcc`** (now improving via `nvidia` channel packages), and it only helps code that runs inside the Conda env — it doesn't give you a system `nvcc` for building arbitrary projects. Pixi and `uv` are newer entrants that lean on the same Conda-package ecosystem.

### 2.5 Per-environment activation hooks & `update-alternatives`

Two lighter tricks appear repeatedly: appending `export PATH=/usr/local/cuda-11.8/bin:$PATH` to a virtualenv's `activate` script (or Conda's `activate.d/`), and Debian/Ubuntu's `update-alternatives` to register competing `cuda` paths. Both work but are manual plumbing, not a managed experience.

### 2.6 Comparison

| Approach | Install/download? | Switch scope | Per-project pin | Cross-platform | Root needed |
|---|---|---|---|---|---|
| `switch-cuda` script | No | Per-shell | No | Linux/macOS only | No |
| Symlink `/usr/local/cuda` | No | Machine-wide | No | Linux | Yes |
| Environment Modules / Lmod | No (admin pre-stages) | Per-shell | No | Linux | To set up |
| Conda / Mamba | **Yes** (runtime) | Per-env | Yes (the env) | Win/Linux/macOS | No |
| venv `activate` hook | No | Per-env | Yes | Both | No |
| Windows PATH reorder (GUI) | No | Machine/user-wide | No | Windows | Admin (system) |
| **Proposed `cvm`** | **Yes (goal)** | **Per-shell + project** | **Yes** | **Win + Linux** | **No (userland)** |

**Takeaway:** every existing tool covers one column well. None covers the whole row. That's the opening.

---

## 3. How `nvm`/`gvm` work — patterns to steal

`nvm` is **not a binary**. It's a POSIX-compliant set of shell functions sourced into your shell from `nvm.sh`. That single design choice is what lets it mutate the *current* shell's `PATH` instantly with no `sudo` and no subprocess boundary. The pieces worth copying:

- **Sourced shell function, not an executable.** A child process can't change its parent's environment, so anything that does `cvm use` must run *in* your shell. `nvm`, `rbenv`, and friends all solve this by being sourced (or by installing a shell shim/hook). This is the most important architectural decision.
- **A versioned install root.** `nvm` keeps everything under `~/.nvm/versions/node/<version>/`. Switching = prepend the chosen version's `bin` to `PATH`. Uninstall = delete a directory. CUDA maps cleanly: `~/.cvm/versions/<cuda-version>/` mirroring NVIDIA's layout (`bin/`, `lib64/` or `lib/x64/`, `include/`).
- **A resolution pipeline.** `nvm` resolves `lts/*`, `stable`, partial versions (`18` → newest `18.x`), and reads `.nvmrc`. `cvm` wants the same: `cvm use 12` picks the newest installed `12.x`; a `.cuda-version` file in the project root auto-selects on `cd`.
- **Project pinning + shell hook.** `.nvmrc` plus an optional `cd` hook gives "right version per directory." Directly portable as `.cuda-version`.
- **Platform split is explicit.** `nvm` (Unix, symlink/PATH) and `nvm-windows` (a separate Go program that flips a symlink at `C:\Program Files\nodejs`) are **two codebases** because the OSes differ too much to share one. Plan for the same split rather than pretending one mechanism fits both.
- **`gvm` adds toolchain-build management** (it can compile Go from source, manage `GOROOT`/`GOPATH`, and namespace dependencies per "pkgset"). The transferable idea is **isolated, named environments**, but CUDA can't be compiled by the user, so this part maps only loosely.

---

## 4. Why CUDA is harder than Node/Go (the constraints that shape the design)

1. **The driver is not yours to swap.** CUDA splits into the **GPU driver** (kernel module, installed system-wide, often admin-managed) and the **CUDA Toolkit** (`nvcc`, runtime libs, headers — userland). A version manager can own the toolkit but must *respect* the driver. The saving grace is **backward compatibility**: a newer driver runs older toolkits (driver 550.x runs CUDA ≤ 12.4; 535.x runs ≤ 12.2, etc.). So the rule the tool must enforce: *you can freely switch toolkits up to the ceiling your installed driver supports.* `cvm` should detect the driver (`nvidia-smi`) and **warn before switching to a toolkit the driver can't support**, optionally hinting at the `cuda-compat` forward-compatibility package.

2. **cuDNN and friends version independently.** cuDNN, cuBLAS, NCCL, TensorRT each have their own version and their own CUDA-compatibility matrix (cuDNN must match the toolkit's major, frequently minor, version). A toolkit-only switch is incomplete for ML users. The tool should at minimum *track and report* the cuDNN paired with each toolkit, and ideally manage cuDNN as a sub-component slotted into the version dir.

3. **Frameworks pin their own CUDA.** PyTorch/TF wheels are compiled against a specific CUDA and ship their own runtime; if the system toolkit diverges by more than a minor version, kernels can fail. This means a CUDA version manager and a Python env manager overlap — `cvm` should stay in its lane (system/build toolkit + `nvcc`) and document that framework wheels are a separate axis.

4. **Windows install model fights userland switching.** Windows toolkits install via MSI to `C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\vXX.X`, set machine/user `CUDA_PATH`, and put `bin` on `PATH`. There's no `source` for `cmd.exe`/PowerShell the way there is for bash. The realistic Windows mechanism is the **`nvm-windows` model**: a small resident program that flips a `CUDA_PATH` value (and a `...\CUDA\current` junction) and relies on a persistent shell profile, or per-session `$env:` injection in PowerShell.

5. **Toolkits are large and the downloads are gated.** A Node tarball is tens of MB; a CUDA toolkit is 2–4 GB, distributed as runfiles/MSI/network installers per OS and per distro. Fully automated install is doable but is the *hard* part — which is why the MVP should switch first, download later.

---

## 5. Proposed design for `cvm`

### 5.1 Philosophy
Own the **toolkit**, respect the **driver**, track the **companion libraries**. Switch per-shell by default (never silently touch system state). Make the common case — "I already installed CUDA 11.8 and 12.4, let me flip between them per project" — one command.

### 5.2 Directory layout
```
~/.cvm/                         (Linux/macOS)   %USERPROFILE%\.cvm\   (Windows)
├── cvm.sh / cvm.ps1            # sourced shell integration
├── versions/
│   ├── 11.8/                   # mirrors NVIDIA layout: bin, lib64|lib/x64, include, nvvm
│   ├── 12.4/
│   └── 12.6/
├── cudnn/                      # optional cuDNN payloads, slotted per toolkit
├── aliases/                    # e.g. "default" -> 12.4, "ml" -> 11.8
└── cache/                      # downloaded installers
```
On Linux the tool can also *adopt* existing `/usr/local/cuda-*` installs by referencing them instead of re-downloading. On Windows it can adopt `C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v*`.

### 5.3 Command surface (model it on nvm)
```
cvm ls                  # installed toolkits, marking active + default
cvm ls-remote           # toolkits available to download (parsed from NVIDIA's index)
cvm install 12.4        # download + unpack into ~/.cvm/versions/12.4
cvm uninstall 11.8
cvm use 12.4            # set CUDA_HOME/PATH/LD_LIBRARY_PATH for THIS shell
cvm use 12              # resolve to newest installed 12.x
cvm use                 # read .cuda-version in cwd
cvm current             # show active toolkit + driver ceiling
cvm alias default 12.4  # persist a default for new shells
cvm which 12.4          # print install path
cvm doctor              # driver vs toolkit check, cuDNN pairing, PATH sanity
cvm exec 11.8 -- nvcc … # run one command under a version without switching
```

### 5.4 The switching mechanism (per platform)
- **Linux/macOS:** ship `cvm.sh`, sourced from `~/.bashrc`/`~/.zshrc`. `cvm use X` exports `CUDA_HOME`, prepends `…/bin` to `PATH`, prepends `…/lib64` to `LD_LIBRARY_PATH`, and *removes any previously-injected cvm paths first* (mirror nvm's `nvm_change_path` cleanup so versions don't stack). A `cd` hook reads `.cuda-version`.
- **Windows:** a resident helper (PowerShell module + optional small binary) that, per session, sets `$env:CUDA_PATH`, `$env:CUDA_HOME`, and reorders `$env:Path`. For "persist as default," update the *user* env vars (never require admin) and flip a `…\CUDA\current` junction so tools hard-coded to a fixed path still resolve.

### 5.5 The `.cuda-version` file
A one-line file (`12.4`, or `12` for "newest 12.x") committed to the repo. On `cd` (or `cvm use` with no arg) the tool resolves and activates it, warning if that version isn't installed and offering `cvm install`. Directly analogous to `.nvmrc`.

### 5.6 Safety rails (the CUDA-specific value-add)
- **Driver ceiling check:** read `nvidia-smi` driver version, refuse-with-warning to switch to a toolkit above what the driver supports, and mention `cuda-compat` as the escape hatch.
- **`cvm doctor`:** verifies `nvcc` matches `CUDA_HOME`, that `LD_LIBRARY_PATH`/`Path` has no stale or duplicate CUDA entries, and that the paired cuDNN (if managed) matches the toolkit major/minor.
- **cuDNN awareness:** at minimum record which cuDNN sits in each version dir and surface it in `cvm ls`; stretch goal is `cvm cudnn install 8.9 --for 12.4`.

### 5.7 Implementation language
A small **Go or Rust** core (for `ls-remote`, downloading, archive extraction, JSON state, Windows env handling) plus thin sourced shell shims is the pragmatic split — exactly the `nvm-windows`/`rbenv` pattern. Pure bash (à la `nvm`) is simplest on Unix but can't serve Windows, which is half your likely audience.

---

## 6. Recommended roadmap

1. **MVP — switch only, Linux + WSL first.** Discover existing toolkits, `ls`/`use`/`current`/`alias`, `.cuda-version`, `doctor` with the driver-ceiling check. No downloading yet. This is `switch-cuda` plus pinning plus safety — shippable and immediately useful, and it's how nvm built trust before it automated installs.
2. **Add `install`/`ls-remote`.** Parse NVIDIA's download index, fetch and unpack into `~/.cvm/versions`. Hardest part; do it after switching is solid.
3. **Windows support** as a parallel track (separate shell integration, `CUDA_PATH` + junction model).
4. **cuDNN / companion-library management** as the differentiator over every existing tool.

---

## 7. Open questions to decide before coding
- **Scope of "version":** toolkit only, or toolkit + cuDNN + NCCL as a bundle? (Bundling is the killer feature but multiplies complexity.)
- **Download legality/automation:** cuDNN historically required a login; toolkits are freely downloadable. Decide whether `cvm` downloads cuDNN or only adopts user-supplied archives.
- **Relationship to Conda/uv/pixi:** position `cvm` as the *system/build* toolkit manager (for `nvcc` and native CUDA projects), explicitly complementary to per-Python-env runtimes, to avoid reinventing Conda.
- **Name collision:** "cvm" is taken several times on GitHub (CMake VM, C/C++ VM, Composer VM, Cortex VM). Consider `cudavm`, `cudaenv`, `nvcc-vm`, or `cudaup` (à la `rustup`).

---

## Sources
- [phohenecker/switch-cuda (bash version-switch script)](https://github.com/phohenecker/switch-cuda)
- [bycloudai/SwapCudaVersionWindows](https://github.com/bycloudai/SwapCudaVersionWindows)
- [Managing multiple CUDA versions using environment modules (Ubuntu)](https://gist.github.com/garg-aayush/156ec6ddda3d62e2c0ddad00b7e66956)
- [MultiCUDA: Multiple Versions of CUDA on One Machine](https://medium.com/@peterjussi/multicuda-multiple-versions-of-cuda-on-one-machine-4b6ccda6faae)
- [Managing Multiple CUDA + cuDNN Installations](https://medium.com/@yushantripleseven/managing-multiple-cuda-cudnn-installations-ba9cdc5e2654)
- [Managing Multiple CUDA Versions on a Single Machine (Towards Data Science)](https://towardsdatascience.com/managing-multiple-cuda-versions-on-a-single-machine-a-comprehensive-guide-97db1b22acdc/)
- [Hamel Husain — CUDA Version Management](https://hamel.dev/notes/cuda.html)
- [nvm-sh/nvm (Node Version Manager)](https://github.com/nvm-sh/nvm)
- [nvm-sh/nvm architecture (DeepWiki)](https://deepwiki.com/nvm-sh/nvm)
- [nvm-windows FAQ](https://www.nvmnode.com/faq/)
- [NVIDIA — CUDA Compatibility (driver vs toolkit, forward compat)](https://docs.nvidia.com/deploy/cuda-compatibility/latest/)
- [CUDA Driver vs CUDA Toolkit explained (TechnoLynx)](https://www.technolynx.com/post/cuda-driver-vs-toolkit-explained)
- [NVIDIA cuDNN Support Matrix](https://docs.nvidia.com/deeplearning/cudnn/backend/latest/reference/support-matrix.html)
- [elenacliu/pytorch_cuda_driver_compatibilities (quick compatibility lookup)](https://github.com/elenacliu/pytorch_cuda_driver_compatibilities)
- [CUDA Installation Guide for Microsoft Windows](https://docs.nvidia.com/cuda/cuda-installation-guide-microsoft-windows/index.html)
