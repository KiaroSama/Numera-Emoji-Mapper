#Requires -Version 7.3
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
# Script-local preferences retain PowerShell pipelines and console ownership while
# preserving literal native arguments and nonzero exit codes independently of the caller.
$PSNativeCommandArgumentPassing = 'Standard'
$PSNativeCommandUseErrorActionPreference = $false
$PSStyle.OutputRendering = 'Ansi'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

function New-LauncherRelaunch(
    [string]$TerminalPath, [string]$PowerShellPath, [string]$ScriptPath,
    [string]$Directory, [string]$Title, [hashtable]$Parameters
) {
    # Only base64 data enters the fixed bootstrap; user strings are never code.
    $json = @{ Script = $ScriptPath; Directory = $Directory; Parameters = $Parameters } |
        ConvertTo-Json -Depth 5 -Compress
    $payload = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json))
    $bootstrap = @'
$ErrorActionPreference = 'Stop'
$data = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__PAYLOAD__')) | ConvertFrom-Json -AsHashtable
Set-Location -LiteralPath $data.Directory
$parameters = $data.Parameters
& $data.Script @parameters -NoRelaunch -PauseOnError
exit $LASTEXITCODE
'@
    $command = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($bootstrap.Replace('__PAYLOAD__', $payload)))
    $psi = [System.Diagnostics.ProcessStartInfo]::new($TerminalPath)
    $psi.UseShellExecute = $false
    # WT splits semicolons inside argv entries; its own escape is backslash-semicolon.
    # Script paths and arguments stay in the encoded data, outside this parser.
    foreach ($arg in @('--title', $Title, $PowerShellPath.Replace(';', '\;'), '-NoLogo', '-NoProfile', '-EncodedCommand', $command)) {
        [void]$psi.ArgumentList.Add($arg)
    }
    return $psi
}

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
            $parameters = @{ Check = [bool]$Check; CommandArguments = $CommandArguments }
            $psi = New-LauncherRelaunch $wt.Source $pwshExe $PSCommandPath $PSScriptRoot 'Numera Emoji Mapper' $parameters
            [void][System.Diagnostics.Process]::Start($psi)
            exit 0
        } catch {
            Write-LauncherConsole WARNING 'Could not launch Windows Terminal; continuing here.'
        }
    }
}

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
    Write-Log INFO "start pwsh=$($PSVersionTable.PSVersion) explorer=$fromExplorer wt=$([bool]$env:WT_SESSION)"
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

if ($shouldPause -and $code -ne 0) {
    Write-Host ''
    Write-LauncherConsole WARNING "Finished with exit code $code. Log: $log"
    Read-Host 'Press Enter to close' | Out-Null
}
exit $code
