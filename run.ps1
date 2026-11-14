<#
.SYNOPSIS
    Opens the standalone Rust menu, or runs its environment doctor with -Check.
.DESCRIPTION
    Resolves the packaged executable or an explicitly built production runtime.
    Never creates an environment, selects an interpreter, installs dependencies,
    or invokes a compiler. Paths and arguments are passed as separate values.
.PARAMETER NoRelaunch
    Accepted for compatibility; the launcher stays attached to this terminal.
.PARAMETER Check
    Runs the non-interactive native doctor and preserves its exit status.
#>
[CmdletBinding()]
param(
    [switch]$NoRelaunch,
    [switch]$Check,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CommandArguments
)

$ErrorActionPreference = 'Stop'
$ScriptRoot = $PSScriptRoot
$suffix = if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) { '.exe' } else { '' }
$candidates = @(
    (Join-Path $ScriptRoot "numera-emoji$suffix"),
    (Join-Path $ScriptRoot "runtime/target/release/numera-emoji$suffix"),
    (Join-Path $ScriptRoot "runtime/target/debug/numera-emoji$suffix")
)
$binary = $null
foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
        $item = Get-Item -LiteralPath $candidate -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            [Console]::Error.WriteLine('Refusing a linked standalone executable.')
            exit 1
        }
        $binary = $candidate
        break
    }
}
if (-not $binary) {
    [Console]::Error.WriteLine('Installed native executable missing. Extract a standalone package, or explicitly build with cargo build --manifest-path runtime/Cargo.toml --locked --release.')
    exit 1
}
if ($Check) {
    if ($CommandArguments.Count -gt 0) {
        [Console]::Error.WriteLine('-Check accepts no command arguments.')
        exit 2
    }
    $CommandArguments = @('doctor')
} elseif (-not $CommandArguments -or $CommandArguments.Count -eq 0) {
    $CommandArguments = @('menu')
}
Push-Location -LiteralPath $ScriptRoot
try {
    & $binary @CommandArguments
    $code = [int]$LASTEXITCODE
} finally {
    Pop-Location
}
exit $code
