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
