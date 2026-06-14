#!/bin/sh
# cuvm installer — download the latest release binary + shell shims and install them.
#
#   curl -LsSf https://raw.githubusercontent.com/danghoangnhan/cuvm/main/install.sh | sh
#
# Knobs (environment variables):
#   CUVM_VERSION      install a specific version, e.g. 0.1.0   (default: latest release)
#   CUVM_INSTALL_DIR  bin dir for the `cuvm` binary  (default: $XDG_BIN_HOME or ~/.local/bin)
#   CUVM_HOME         cuvm data dir; shims land in $CUVM_HOME/shims  (default: ~/.cuvm)
#   CUVM_NO_MODIFY_PATH  set to any value to skip the PATH/shim hint at the end
#   CUVM_DOWNLOAD_BASE   release-asset base URL  (default: GitHub releases; override
#                        for a mirror/air-gapped host serving v<ver>/<asset> paths)
#
# POSIX sh — no bashisms; runs under dash/ash/busybox.
set -eu

REPO="danghoangnhan/cuvm"
RELEASES="https://github.com/${REPO}/releases"
API_LATEST="https://api.github.com/repos/${REPO}/releases/latest"
DL_BASE="${CUVM_DOWNLOAD_BASE:-${RELEASES}/download}"

say() { printf 'cuvm: %s\n' "$1"; }
err() { printf 'cuvm: error: %s\n' "$1" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# --- pick a downloader (curl or wget) ----------------------------------------
if have curl; then
  dl() { curl -fsSL "$1" -o "$2"; }
  dl_stdout() { curl -fsSL "$1"; }
elif have wget; then
  dl() { wget -qO "$2" "$1"; }
  dl_stdout() { wget -qO- "$1"; }
else
  err "need curl or wget to download cuvm"
fi

# --- detect platform → release asset name ------------------------------------
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Linux) ;;
  Darwin) err "macOS has no cuvm binary (CUDA is Linux/Windows only). See ${RELEASES}." ;;
  *) err "unsupported OS '$os'. Prebuilt targets: linux-amd64, linux-arm64, windows-amd64 (use install.ps1 on Windows)." ;;
esac
case "$arch" in
  x86_64 | amd64) name="linux-amd64" ;;
  aarch64 | arm64) name="linux-arm64" ;;
  *) err "unsupported architecture '$arch' on Linux (have: x86_64, aarch64)." ;;
esac

# --- resolve version ---------------------------------------------------------
ver="${CUVM_VERSION:-}"
if [ -z "$ver" ]; then
  say "resolving the latest release…"
  # Pull tag_name out of the GitHub API JSON without a jq dependency.
  ver="$(dl_stdout "$API_LATEST" \
    | grep -m1 '"tag_name"' \
    | sed -e 's/.*"tag_name"[[:space:]]*:[[:space:]]*"//' -e 's/".*//')"
  [ -n "$ver" ] || err "could not determine the latest release version from ${API_LATEST} (set CUVM_VERSION to override)."
fi
ver="${ver#v}" # accept either "0.1.0" or "v0.1.0"

stage="cuvm-${ver}-${name}"
archive="${stage}.tar.gz"
url="${DL_BASE}/v${ver}/${archive}"

# --- download into a scratch dir (cleaned on exit) ---------------------------
tmp="$(mktemp -d "${TMPDIR:-/tmp}/cuvm-install.XXXXXX")"
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT INT TERM

say "downloading ${archive}…"
dl "$url" "${tmp}/${archive}" || err "download failed: ${url}"

# --- verify the checksum if SHA256SUMS is published --------------------------
if dl "${DL_BASE}/v${ver}/SHA256SUMS" "${tmp}/SHA256SUMS" 2>/dev/null; then
  want="$(grep " ${archive}\$" "${tmp}/SHA256SUMS" | awk '{print $1}')"
  if [ -n "$want" ]; then
    if have sha256sum; then got="$(sha256sum "${tmp}/${archive}" | awk '{print $1}')"
    elif have shasum; then got="$(shasum -a 256 "${tmp}/${archive}" | awk '{print $1}')"
    else got=""; say "no sha256 tool found; skipping checksum verification"; fi
    if [ -n "$got" ] && [ "$got" != "$want" ]; then
      err "checksum mismatch for ${archive} (expected ${want}, got ${got})"
    fi
    [ -n "$got" ] && say "checksum OK"
  fi
else
  say "SHA256SUMS not published for this release; skipping checksum verification"
fi

# --- unpack + install --------------------------------------------------------
tar xzf "${tmp}/${archive}" -C "$tmp"
[ -f "${tmp}/${stage}/cuvm" ] || err "archive did not contain ${stage}/cuvm"

bin_dir="${CUVM_INSTALL_DIR:-${XDG_BIN_HOME:-$HOME/.local/bin}}"
cuvm_home="${CUVM_HOME:-$HOME/.cuvm}"
mkdir -p "$bin_dir" "${cuvm_home}/shims"
install -m 0755 "${tmp}/${stage}/cuvm" "${bin_dir}/cuvm"
if [ -d "${tmp}/${stage}/shims" ]; then
  cp "${tmp}/${stage}/shims/"* "${cuvm_home}/shims/" 2>/dev/null || true
fi

say "installed cuvm ${ver} → ${bin_dir}/cuvm"

# --- next steps --------------------------------------------------------------
if [ -n "${CUVM_NO_MODIFY_PATH:-}" ]; then
  exit 0
fi

# Is the bin dir already reachable on PATH?
on_path=no
case ":${PATH}:" in *":${bin_dir}:"*) on_path=yes ;; esac

printf '\n'
say "next steps:"
if [ "$on_path" = no ]; then
  printf '  1. add cuvm to your PATH:\n'
  printf '       export PATH="%s:$PATH"\n' "$bin_dir"
  printf '  2. enable shell integration (cd-autoload + the cuvm wrapper):\n'
else
  printf '  enable shell integration (cd-autoload + the cuvm wrapper):\n'
fi
printf '       # bash  → add to ~/.bashrc\n'
printf '       source %s/shims/cuvm.sh\n' "$cuvm_home"
printf '       # zsh   → add to ~/.zshrc\n'
printf '       source %s/shims/cuvm.zsh\n' "$cuvm_home"
printf '\n'
printf '  then restart your shell (or re-source the file) and run: cuvm --help\n'
