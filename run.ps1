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
# Keep the native command tail outside PowerShell's named/common parameter binder.
# Only leading launcher controls are parsed below; later tokens are literal data.

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
    $Check = $false
    $CommandArguments = @()
    $seenCheck = $false
    $seenArguments = $false
    $index = 0
    while ($index -lt $args.Count) {
        $option = $args[$index]
        if ($option -isnot [string]) { break }
        if ($option -ieq '-Check' -or $option -ieq '-Check:') {
            if ($seenCheck) {
                $failureMessage = '-Check may be specified only once.'
                throw $failureMessage
            }
            $seenCheck = $true
            $Check = $true
            $index++
            # A caller/hashtable splat keeps the colon; -File removes it but
            # retains the Boolean value for -Check:$true and -Check:$false.
            if ($index -lt $args.Count -and ($args[$index] -is [bool] -or $args[$index] -is [switch])) {
                $Check = [bool]$args[$index]
                $index++
            } elseif ($option.EndsWith(':')) {
                $failureMessage = '-Check requires a Boolean switch value.'
                throw $failureMessage
            }
        } elseif ($option -ieq '-CommandArguments' -or $option -ieq '-CommandArguments:') {
            if ($seenArguments -or $index + 1 -ge $args.Count) {
                $failureMessage = '-CommandArguments requires one value and may be specified only once.'
                throw $failureMessage
            }
            $seenArguments = $true
            $index++
            # Match the old typed array contract: scalar null means no args;
            # null elements inside an array become literal empty strings.
            if ($null -eq $args[$index]) { $CommandArguments = @() }
            else { $CommandArguments = [string[]]$args[$index] }
            $index++
        } else {
            break
        }
    }
    if ($index -lt $args.Count) {
        $remaining = $args[$index..($args.Count - 1)]
        # PowerShell unwraps one positional array, but converts arrays among
        # several positional values to strings. Preserve that binding contract.
        if ($remaining.Count -eq 1 -and $remaining[0] -is [array]) {
            $remaining = $remaining[0]
        }
        $CommandArguments += [string[]]$remaining
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
