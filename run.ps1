#Requires -Version 7.3
<#
.SYNOPSIS
    Opens the Numera Emoji Mapper menu, runs the environment doctor, or passes any
    other command straight to the native executable.
.DESCRIPTION
    Resolves bin\numera-emoji.exe next to this script, so it works from any
    working directory and from a path with spaces. Runs in the original console.
    When launched from Explorer in an interactive console, waits for Enter after
    a failure so the error stays readable.
.EXAMPLE
    .\run.ps1              # interactive menu
.EXAMPLE
    .\run.ps1 -Check       # environment doctor, exit code 0 or 1
.EXAMPLE
    .\run.ps1 status       # any native command
#>
param(
    [switch]$Check,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CommandArguments = @()
)

$ErrorActionPreference = 'Stop'
# Script-local preferences retain PowerShell pipelines and console ownership while
# preserving literal native arguments and nonzero exit codes independently of the caller.
$PSNativeCommandArgumentPassing = 'Standard'
$PSNativeCommandUseErrorActionPreference = $false
$PSStyle.OutputRendering = 'Ansi'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

try { [Console]::Title = 'Numera Emoji Mapper' } catch {}

function Write-LauncherConsole(
    [ValidateSet('INFO', 'WARNING', 'ERROR', 'DEBUG')][string]$Level,
    [string]$Message
) {
    $time = [DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss')
    $color = switch ($Level) { 'ERROR' { 'Red' }; 'WARNING' { 'Yellow' }; default { 'Cyan' } }
    Write-Host "[$time UTC] [$Level] [run] $Message" -ForegroundColor $color
}

function Disable-LauncherLog {
    if ($script:logDisabled) { return }
    $script:logDisabled = $true
    $script:log = $null
    Write-LauncherConsole WARNING 'Launcher logging disabled; continuing without a log file.'
}

function Write-Log(
    [ValidateSet('INFO', 'WARNING', 'ERROR', 'DEBUG')][string]$Level,
    [string]$Message
) {
    if (-not $script:log) { return }
    $time = [DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss')
    try {
        # Arguments and serialized payloads must never enter the log.
        Add-Content -LiteralPath $script:log -Encoding utf8 -Value "[$time UTC] [$Level] [run] $Message" -ErrorAction Stop
    } catch {
        Disable-LauncherLog
    }
}

$binary = Join-Path $PSScriptRoot 'bin' 'numera-emoji.exe'
$logDir = Join-Path $PSScriptRoot 'logs'
$log = $null
$logDisabled = $false
try {
    [void][System.IO.Directory]::CreateDirectory($logDir)
    $stamp = [DateTime]::UtcNow.ToString('yyyy-MM-dd_HH-mm-ss')
    $log = Join-Path $logDir "run_${stamp}_UTC_$PID.log"
} catch {
    Disable-LauncherLog
}

$code = 1
$failureMessage = 'Could not start the native executable.'
try {
    Write-Log INFO "start pwsh=$($PSVersionTable.PSVersion)"
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        $failureMessage = 'Native executable not found. Put the signed numera-emoji.exe from a release package in the bin folder.'
        throw $failureMessage
    }
    if ($Check) {
        if ($CommandArguments.Count -gt 0) {
            $failureMessage = '-Check accepts no command arguments.'
            throw $failureMessage
        }
        $nativeArguments = @('doctor')
    } elseif ($CommandArguments.Count -gt 0) {
        $nativeArguments = $CommandArguments
    } else {
        $nativeArguments = @('menu')
    }
    Write-Log DEBUG 'Starting the native process with inherited console streams.'
    & $binary @nativeArguments
    $code = [int]$LASTEXITCODE
    Write-Log INFO "exit code=$code"
} catch {
    Write-LauncherConsole ERROR $failureMessage
    Write-Log ERROR $failureMessage
    $code = 1
}

if ($code -ne 0 -and -not [Console]::IsInputRedirected -and -not [Console]::IsOutputRedirected -and -not [Console]::IsErrorRedirected) {
    $parentProc = (Get-Process -Id $PID -ErrorAction SilentlyContinue).Parent
    if ($null -eq $parentProc -or $parentProc.ProcessName -eq 'explorer') {
        $message = "Finished with exit code $code."
        if ($log) { $message += " Log: $log" }
        Write-LauncherConsole WARNING $message
        try { Read-Host 'Press Enter to close' | Out-Null } catch {}
    }
}
exit $code
