# crates/cuvm-cli/tests/fixtures/run_shim_zsh.sh
# Args identical to the bash driver; $3 = path to cuvm.zsh shim.
set -eu
export PATH="$1:$PATH"
export CUVM_HOME="$2"
# shellcheck disable=SC1090
source "$3"

cd "$4"
__cuvm_autoload
echo "CUDA_HOME_AFTER_PIN=$CUDA_HOME"
echo "CURRENT_AFTER_PIN=${CUVM_CURRENT:-}"

__cuvm_autoload
_dups="$(printf '%s' "$PATH" | tr ':' '\n' | grep -c "$CUDA_HOME/bin" || true)"
echo "PATH_BIN_COUNT=$_dups"

cd "$5"
__cuvm_autoload
echo "CURRENT_AFTER_LEAVE=${CUVM_CURRENT:-}"
