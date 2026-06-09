//! `cuvm hook --shell <s>` — emit the cd-autoload glue to stdout.

use anyhow::Result;
use cuvm_core::{Os, Shell};

/// Emit the cd-autoload hook glue for the given shell to stdout.
///
/// Pure adapter: dispatch to the runtime `Activator` (WU-5 impl) and print.
///
/// # Errors
/// Propagates any error from the activator's `hook` method.
pub fn run(shell: Shell, os: Os) -> Result<()> {
    let activator = cuvm_platform::new_activator(os);
    let script = activator.hook(shell)?;
    print!("{script}");
    Ok(())
}
