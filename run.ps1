#Requires -Version 7.0
<#
.SYNOPSIS
    Opens the Numera Emoji Mapper menu, runs the environment doctor, or passes any
    other command straight to the native executable.
.DESCRIPTION
    Resolves bin\numera-emoji.exe next to this script, so it works from any
    working directory and from a path with spaces. When started by a double-click
    in Explorer, relaunches inside Windows Terminal (when available, with a
    loop guard) for full color rendering and waits for Enter after a failure so
    the error stays readable.
.EXAMPLE
    .\run.ps1              # interactive menu
.EXAMPLE
    .\run.ps1 -Check       # environment doctor, exit code 0 or 1
.EXAMPLE
    .\run.ps1 status       # any native command
#>
param(
    [switch]$Check,
    [switch]$PauseOnError,
    [switch]$NoRelaunch,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CommandArguments = @()
)

$ErrorActionPreference = 'Stop'
$PSStyle.OutputRendering = 'Ansi'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

$binary = Join-Path $PSScriptRoot 'bin' 'numera-emoji.exe'
$interactiveConsole = (-not [Console]::IsInputRedirected) -and (-not [Console]::IsOutputRedirected)
$parentProc = (Get-Process -Id $PID -ErrorAction SilentlyContinue).Parent
$fromExplorer = $interactiveConsole -and (($null -eq $parentProc) -or ($parentProc.ProcessName -eq 'explorer'))
$shouldPause = $PauseOnError -or $fromExplorer

# Explorer starts double-clicked scripts in legacy conhost; prefer Windows Terminal once.
if ($fromExplorer -and -not $NoRelaunch -and -not $env:WT_SESSION) {
    $wt = Get-Command 'wt.exe' -ErrorAction SilentlyContinue
    if ($wt) {
        try {
            $pwshExe = (Get-Process -Id $PID).Path
            $psi = [System.Diagnostics.ProcessStartInfo]::new($wt.Source)
            $psi.UseShellExecute = $false
            foreach ($arg in @('-d', $PSScriptRoot, '--title', 'Numera Emoji Mapper', $pwshExe, '-NoLogo', '-NoProfile', '-File', $PSCommandPath, '-NoRelaunch', '-PauseOnError')) {
                [void]$psi.ArgumentList.Add($arg)
            }
            if ($Check) { [void]$psi.ArgumentList.Add('-Check') }
            foreach ($arg in $CommandArguments) { [void]$psi.ArgumentList.Add($arg) }
            [void][System.Diagnostics.Process]::Start($psi)
            exit 0
        } catch {
            Write-Host '[!] Could not launch Windows Terminal; continuing here.' -ForegroundColor Yellow
        }
    }
}

$logDir = Join-Path $PSScriptRoot 'logs'
$log = $null
try {
    New-Item -ItemType Directory -Force -Path $logDir | Out-Null
    $stamp = [DateTime]::UtcNow.ToString('yyyy-MM-dd_HH-mm-ss')
    $log = Join-Path $logDir "run_${stamp}_UTC_$PID.log"
} catch {
    Write-Host "[!] Launcher log disabled: $($_.Exception.Message)" -ForegroundColor Yellow
}
function Write-Log([string]$Level, [string]$Message) {
    if (-not $script:log) { return }
    $time = [DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss')
    # Arguments are never logged: a command line can carry an id or a token.
    Add-Content -LiteralPath $script:log -Encoding utf8 -Value "[$time UTC] [$Level] [run] $Message"
}

$code = 1
try {
    Write-Log INFO "start pwsh=$($PSVersionTable.PSVersion) explorer=$fromExplorer wt=$([bool]$env:WT_SESSION) binary=$binary"
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        throw "Native executable not found: $binary. Put the signed numera-emoji.exe from a release package in the bin folder."
    }
    if ($Check) {
        if ($CommandArguments.Count -gt 0) { throw '-Check accepts no command arguments.' }
        & $binary doctor
    } elseif ($CommandArguments.Count -gt 0) {
        & $binary @CommandArguments
    } else {
        & $binary menu
    }
    $code = [int]$LASTEXITCODE
    Write-Log INFO "exit code=$code"
} catch {
    Write-Host "[X] $($_.Exception.Message)" -ForegroundColor Red
    Write-Log ERROR $_.Exception.Message
    $code = 1
}

if ($shouldPause -and $code -ne 0) {
    Write-Host ''
    Write-Host "[!] Finished with exit code $code. Log: $log" -ForegroundColor Yellow
    Read-Host 'Press Enter to close' | Out-Null
}
exit $code
