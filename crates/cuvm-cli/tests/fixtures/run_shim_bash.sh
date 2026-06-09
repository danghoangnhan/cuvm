# crates/cuvm-cli/tests/fixtures/run_shim_bash.sh
# Args: $1 = path to cuvm binary dir (prepended to PATH), $2 = CUVM_HOME,
#       $3 = path to cuvm.sh shim, $4 = pinned fixture dir, $5 = unpinned dir.
set -eu
export PATH="$1:$PATH"
export CUVM_HOME="$2"
# shellcheck disable=SC1090
. "$3"

# 1) enter pinned dir -> autoload activates the pinned toolkit
cd "$4"
__cuvm_autoload
echo "CUDA_HOME_AFTER_PIN=$CUDA_HOME"
echo "CURRENT_AFTER_PIN=${CUVM_CURRENT:-}"

# 2) re-activate -> PATH must not gain a second copy of the injected bin
__cuvm_autoload
_dups="$(printf '%s' "$PATH" | tr ':' '\n' | grep -c "$CUDA_HOME/bin" || true)"
echo "PATH_BIN_COUNT=$_dups"

# 3) leave pinned dir -> revert to the default alias
cd "$5"
__cuvm_autoload
echo "CURRENT_AFTER_LEAVE=${CUVM_CURRENT:-}"
