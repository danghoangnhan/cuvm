# shims/cuvm.sh — sourced from ~/.bashrc. Defines the cuvm() wrapper + autoload hook.
# The binary prints shell code to stdout; this function eval's it (print-then-eval).

cuvm() {
  case "${1:-}" in
    use|env|shell|default)
      # mutating verbs: capture stdout (env script) and eval it in this shell.
      eval "$(command cuvm "$@" --shell bash)"
      ;;
    *)
      command cuvm "$@"
      ;;
  esac
}

# cd-autoload: re-activate from .cuda-version (upward walk done by the binary),
# or revert to the persistent default when no pin is in scope. Tracks the last
# directory we acted on so we only re-emit env when the pin context changes.
__cuvm_autoload() {
  # `cuvm env` with no spec resolves from cwd (.cuda-version, else default);
  # eval applies it. Diagnostics go to stderr and are left visible.
  local _script
  _script="$(command cuvm env --shell bash 2>/dev/null)" || return 0
  [ -n "$_script" ] && eval "$_script"
}
