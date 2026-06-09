# cuvm — nvm for CUDA

`cuvm` is an [nvm](https://github.com/nvm-sh/nvm)-style version manager for the
**CUDA toolkit** (and cuDNN). Install, switch, and pin multiple CUDA toolkits
per-shell, with no root and zero runtime dependencies. Linux / WSL **and**
Windows.

> **Status — Milestone 1.** This release ships *adopt / switch / pin / doctor*
> with **no downloading**: it manages CUDA toolkits already on your machine.
> Installing toolkits from NVIDIA's redistributables lands in M2.

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
cuvm adopt /usr/local/cuda-12.4   # register an existing toolkit in place
cuvm adopt --scan                 # discover & adopt /usr/local/cuda-* installs
cuvm ls                           # list managed toolkits (default marked *)
cuvm use 12.4                     # activate in the current shell
cuvm default 12.6                 # set the persistent default
cuvm pin 12.4                     # write .cuda-version in the current dir
cuvm which 12.4                   # print a toolkit's absolute root
cuvm doctor                       # diagnose driver/toolkit/PATH health
```

`adopt` never moves or deletes your existing installs — it registers them in
place (`~/.cuvm`) and `uninstall` only de-registers adopted toolkits.

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
