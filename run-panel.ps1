#Requires -Version 7.0
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
    Also show the emoji already live in these packs, for example -WithPack 6.
.PARAMETER All
    Also show every emoji already live in a pack.
.EXAMPLE
    .\run-panel.ps1
.EXAMPLE
    .\run-panel.ps1 -WithPack 6
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
            foreach ($arg in @('-d', $PSScriptRoot, '--title', 'Numera Emoji Mapper - Curate Panel', $pwshExe, '-NoLogo', '-NoProfile', '-File', $PSCommandPath, '-NoRelaunch', '-PauseOnError')) {
                [void]$psi.ArgumentList.Add($arg)
            }
            if ($WithPack.Count -gt 0) {
                [void]$psi.ArgumentList.Add('-WithPack')
                [void]$psi.ArgumentList.Add(($WithPack -join ','))
            }
            if ($All) { [void]$psi.ArgumentList.Add('-All') }
            foreach ($arg in $ExtraArgs) { [void]$psi.ArgumentList.Add($arg) }
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
    $log = Join-Path $logDir "run-panel_${stamp}_UTC_$PID.log"
} catch {
    Write-Host "[!] Launcher log disabled: $($_.Exception.Message)" -ForegroundColor Yellow
}
function Write-Log([string]$Level, [string]$Message) {
    if (-not $script:log) { return }
    $time = [DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss')
    Add-Content -LiteralPath $script:log -Encoding utf8 -Value "[$time UTC] [$Level] [run-panel] $Message"
}

$code = 1
try {
    Write-Log INFO "start pwsh=$($PSVersionTable.PSVersion) explorer=$fromExplorer wt=$([bool]$env:WT_SESSION) binary=$binary"
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        throw "Native executable not found: $binary. Put the signed numera-emoji.exe from a release package in the bin folder."
    }
    $panelArgs = @('panel')
    foreach ($pack in $WithPack) {
        if ($pack -lt 1) { throw "-WithPack takes pack numbers from 1 up; got $pack." }
        $panelArgs += @('--with-pack', "$pack")
    }
    if ($All) { $panelArgs += '--all' }
    $panelArgs += $ExtraArgs

    $e = [char]27
    Write-Host ''
    Write-Host "$e[1;38;2;255;50;115mNumera Emoji Mapper - Curate Panel$e[0m"
    Write-Host "$e[1;38;2;255;50;115m==================================$e[0m"
    Write-Host '[*] Starting the Curate panel. Keep this window open; press Ctrl+C to stop it.' -ForegroundColor Cyan
    & $binary @panelArgs
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
