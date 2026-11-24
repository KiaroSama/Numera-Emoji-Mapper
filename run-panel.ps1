#Requires -Version 7.3
<#
.SYNOPSIS
    Opens the Curate panel: the browser page where emoji are sorted into packs.
.DESCRIPTION
    Starts `numera-emoji panel` from bin\numera-emoji.exe next to this script and
    opens the browser. When started by a double-click in Explorer, relaunches
    inside Windows Terminal (when available, with a loop guard) so status output
    stays colorful and readable, keeps the window open while the panel runs
    (Ctrl+C stops it), and waits for Enter if the panel exits with an error.
.PARAMETER WithPack
    Also show the emoji already live in these packs, for example -WithPack 6,7,8.
.PARAMETER All
    Also show every emoji already live in a pack.
.EXAMPLE
    .\run-panel.ps1
.EXAMPLE
    .\run-panel.ps1 -WithPack 6
.EXAMPLE
    .\run-panel.ps1 -WithPack 6,7,8
#>
param(
    [int[]]$WithPack = @(),
    [switch]$All,
    [switch]$PauseOnError,
    [switch]$NoRelaunch,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ExtraArgs = @()
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
    Write-Host "[$time UTC] [$Level] [run-panel] $Message" -ForegroundColor $color
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
        Add-Content -LiteralPath $script:log -Encoding utf8 -Value "[$time UTC] [$Level] [run-panel] $Message" -ErrorAction Stop
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
            $parameters = @{ WithPack = $WithPack; All = [bool]$All; ExtraArgs = $ExtraArgs }
            $psi = New-LauncherRelaunch $wt.Source $pwshExe $PSCommandPath $PSScriptRoot 'Numera Emoji Mapper - Curate Panel' $parameters
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
    $log = Join-Path $logDir "run-panel_${stamp}_UTC_$PID.log"
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
    $panelArgs = @('panel')
    foreach ($pack in $WithPack) {
        if ($pack -lt 1) {
            $failureMessage = '-WithPack takes pack numbers from 1 up.'
            throw $failureMessage
        }
        $panelArgs += @('--with-pack', "$pack")
    }
    if ($All) { $panelArgs += '--all' }
    $panelArgs += $ExtraArgs

    $e = [char]27
    Write-Host ''
    Write-Host "$e[1;38;2;255;50;115mNumera Emoji Mapper - Curate Panel$e[0m"
    Write-Host "$e[1;38;2;255;50;115m==================================$e[0m"
    Write-Host '[*] Starting the Curate panel. Keep this window open; press Ctrl+C to stop it.' -ForegroundColor Cyan
    Write-Log DEBUG 'Starting the native process with inherited console streams.'
    & $binary @panelArgs
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
