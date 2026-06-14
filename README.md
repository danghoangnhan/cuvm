# cuvm — nvm for CUDA

`cuvm` is an [nvm](https://github.com/nvm-sh/nvm)-style version manager for the
**CUDA toolkit** (and cuDNN). Install, switch, and pin multiple CUDA toolkits
per-shell, with no root and zero runtime dependencies. Linux / WSL **and**
Windows.

> **Status — Milestone 4 (in progress).** Activation polish has landed:
> `cuvm exec <spec> -- <cmd>` runs a one-off command with a toolkit active,
> `cuvm shell <spec>` drops into a subshell with it active, `cuvm completions
> <shell>` emits shell completions, and `ls-remote` takes a version filter plus
> `--all-versions`/`--show-urls`. Built on M3's cuDNN pairing: `install` pairs
> each toolkit with a matching cuDNN via EULA-gated auto-download from NVIDIA's
> account-free cuDNN redist (user-supplied archives can always be ingested
> instead), payloads live in a content-addressed store
> (`~/.cuvm/cudnn/<sha256>/`) linked into the toolkit tree, and `doctor`
> validates the toolkit↔cuDNN pairing. NCCL companion libs are the remaining M4
> work.

## Install

Download the archive for your platform from the
[latest release](https://github.com/danghoangnhan/cuvm/releases/latest),
verify it against `SHA256SUMS`, unpack it, and put `cuvm` on your `PATH`.

```sh
# Linux x86_64 (musl, static)
curl -fsSLO https://github.com/danghoangnhan/cuvm/releases/latest/download/cuvm-<ver>-linux-amd64.tar.gz
tar xzf cuvm-<ver>-linux-amd64.tar.gz
install -Dm755 cuvm-<ver>-linux-amd64/cuvm ~/.local/bin/cuvm
```

Prebuilt targets: `linux-amd64` (musl, static), `linux-arm64`, `windows-amd64`.

### Shell integration

`cuvm` activates a toolkit by printing an env script your shell `eval`s. Source
the shim for your shell (bundled in the archive under `shims/`):

```sh
# bash — add to ~/.bashrc
source /path/to/shims/cuvm.sh
# zsh — add to ~/.zshrc
source /path/to/shims/cuvm.zsh
```

```powershell
# PowerShell — add to $PROFILE
. "C:\path\to\shims\cuvm.ps1"
```

The shim wires up the `cuvm` wrapper and a `cd`-autoload hook that re-activates
from a directory's `.cuda-version` pin.

## Usage

```sh
cuvm install 12.4 12.6            # download & install one or more toolkits
cuvm install -r 12.4              # reinstall even if present (replace the existing install)
cuvm install 12.4 --accept-eula   # toolkit + paired cuDNN (EULA recorded once)
cuvm cudnn install 9.8 --for 12.4.1            # pair/retrofit a specific cuDNN
cuvm cudnn install ./cudnn-*.tar.xz --for 12.4.1   # air-gapped: ingest a local archive
cuvm cudnn ls                     # cuDNN payloads in the content store
cuvm adopt /usr/local/cuda-12.4   # register an existing toolkit in place
cuvm adopt --scan                 # discover & adopt /usr/local/cuda-* installs
cuvm ls                           # installed toolkits + `<download available>`
cuvm ls --output-format json      # the same list, machine-readable
cuvm ls-remote                    # downloadable versions (alias: ls --only-downloads)
cuvm ls-remote 12.4 --all-versions   # filter remote versions; show every patch
cuvm ls-remote --cudnn            # downloadable cuDNN versions
cuvm ls-remote --nccl             # downloadable NCCL versions
cuvm use 12.4                     # activate in the current shell
cuvm exec 12.4 -- nvcc --version  # run one command with 12.4 active (no shell switch)
cuvm shell 12.4                   # drop into a subshell with 12.4 active (exit to return)
cuvm default 12.6                 # set the persistent default
cuvm pin 12.4                     # write .cuda-version in the current dir
cuvm which 12.4                   # print a toolkit's absolute root
cuvm doctor                       # diagnose driver/toolkit/PATH health
cuvm uninstall 12.4.1             # remove a toolkit (exact handle, see cuvm ls)
cuvm completions zsh              # print a shell completion script (bash/zsh/fish/pwsh/elvish)
```

`install` is idempotent — re-running it on an installed version is a no-op
unless you pass `--reinstall`/`-r`. Installing a version newer than your
driver's ceiling is refused unless you pass `--force`.

`adopt` never moves or deletes your existing installs — it registers them in
place (`~/.cuvm`) and `uninstall` only de-registers adopted toolkits.

cuDNN auto-download is gated behind a one-time acceptance of the NVIDIA cuDNN
EULA — pass `--accept-eula` or answer the interactive prompt once, and the
acceptance is recorded under `~/.cuvm/eula/`. User-supplied archives
(`cuvm cudnn install ./cudnn-*.tar.xz --for …`) are always accepted, with no
network access required.

`exec`/`shell` apply the same per-shell activation as `use` (strip the prior
`CUVM_INJECTED` breadcrumb, then prepend the toolkit's `bin`/`lib64`) directly
to a child process, so a one-off command or subshell gets an activated CUDA
environment without touching the parent shell. NCCL discovery has landed
(`cuvm ls-remote --nccl`, sourced from NVIDIA's account-free NCCL redist, which
ships no manifest or checksums — cuvm self-records each archive's sha256);
`cuvm nccl install` pairing is the remaining M4 work.

## Building from source

```sh
cargo build --release -p cuvm-cli
# cross-compile (matches CI / releases):
cargo zigbuild -p cuvm-cli --release --target x86_64-pc-windows-gnu
```

Requires Rust 1.92+. Cross-compilation uses [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild).

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
