# shims/cuvm.zsh — sourced from ~/.zshrc. Defines the cuvm() wrapper + autoload hook.
# The binary prints shell code to stdout; this function eval's it (print-then-eval).

cuvm() {
  case "${1:-}" in
    use|env|shell|default)
      eval "$(command cuvm "$@" --shell bash)"
      ;;
    *)
      command cuvm "$@"
      ;;
  esac
}

__cuvm_autoload() {
  local _script
  _script="$(command cuvm env --shell bash 2>/dev/null)" || return 0
  [ -n "$_script" ] && eval "$_script"
}
