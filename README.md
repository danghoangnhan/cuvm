# cuvm — nvm for CUDA

`cuvm` is an [nvm](https://github.com/nvm-sh/nvm)-style version manager for the
**CUDA toolkit** (plus cuDNN, NCCL, and cuBLAS math libs). Install, switch, and
pin multiple CUDA toolkits per-shell — no root, zero runtime dependencies, on
Linux / WSL **and** Windows.

## Install

**Linux / WSL**

```sh
curl -LsSf https://raw.githubusercontent.com/danghoangnhan/cuvm/main/install.sh | sh
```

**Windows (PowerShell)**

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://raw.githubusercontent.com/danghoangnhan/cuvm/main/install.ps1 | iex"
```

Then source the shim once (`source ~/.cuvm/shims/cuvm.sh` in your `~/.bashrc`) to
enable the `cuvm` wrapper and `cd`-autoload. See the
[Installation guide](https://github.com/danghoangnhan/cuvm/wiki/Installation) for
manual install, version pinning, env knobs, and shell integration.

## Quick start

```sh
cuvm install 12.4        # download a CUDA toolkit
cuvm use 12.4            # activate it in the current shell
cuvm pin 12.4            # write .cuda-version so cd auto-activates here
cuvm doctor              # check driver / PATH / pairing health
```

## Documentation

Full docs live in the **[wiki](https://github.com/danghoangnhan/cuvm/wiki)**:

- **[Installation](https://github.com/danghoangnhan/cuvm/wiki/Installation)** — one-line & manual install, env knobs, shell integration
- **[Usage](https://github.com/danghoangnhan/cuvm/wiki/Usage)** — commands, switching, pinning, `doctor`, `.cuda-version`
- **[Companion libraries](https://github.com/danghoangnhan/cuvm/wiki/Companion-Libraries)** — cuDNN, NCCL, cuBLAS math libs
- **[Building from source](https://github.com/danghoangnhan/cuvm/wiki/Building-from-Source)** — native + cross-compile

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
