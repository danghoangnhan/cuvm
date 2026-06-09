# cuvm.ps1 — PowerShell shim. Dot-source from $PROFILE: . "$HOME\.cuvm\shims\cuvm.ps1"
# The binary prints shell code to stdout; activation verbs get Invoke-Expression'd.
function cuvm {
    if ($args.Count -gt 0 -and ($args[0] -in 'use','env','shell','default')) {
        (& cuvm.exe @args --shell powershell | Out-String) | Invoke-Expression
    } else {
        & cuvm.exe @args
    }
}
