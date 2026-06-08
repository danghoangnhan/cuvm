//! Unit tests that assert the embedded shim strings contain the load-bearing
//! protocol lines required by WU-6.

use cuvm_cli::shims;

#[test]
fn bash_shim_defines_function_and_eval_protocol() {
    let s = shims::BASH_SHIM;
    assert!(
        s.contains("cuvm()"),
        "bash shim must define cuvm() function"
    );
    // dispatch the four mutating verbs through eval of `command cuvm ... --shell bash`
    assert!(s.contains("use|env|shell|default"));
    assert!(s.contains(r#"eval "$(command cuvm "$@" --shell bash)""#));
    // passthrough for everything else
    assert!(s.contains(r#"command cuvm "$@""#));
    // the hook function the `cuvm hook` output chains into
    assert!(s.contains("__cuvm_autoload"));
}

#[test]
fn zsh_shim_uses_bash_emitter_and_defines_function() {
    let s = shims::ZSH_SHIM;
    assert!(s.contains("cuvm()"));
    // zsh emits with --shell bash too (bash/zsh env syntax is identical here)
    assert!(s.contains(r#"eval "$(command cuvm "$@" --shell bash)""#));
    assert!(s.contains("__cuvm_autoload"));
}
