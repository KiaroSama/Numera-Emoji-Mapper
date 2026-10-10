# Watchdog driver for the crypto-coin SVG->PNG conversion.
#
# The converter writes the name of the file it is working on into a marker file
# and clears it afterwards, so the marker is a per-file heartbeat. The watchdog
# gives every single file 120 seconds; it no longer watches the output-file
# count, which killed a legitimately slow (or blank-result) source just because
# no new PNG had appeared for 30 seconds.
#
# A killed file is quarantined by the next converter run and reported here for
# REVIEW. Exits non-zero when the conversion never completed, when a file was
# quarantined, or when the converter itself reports failures -- never pretend a
# dead run succeeded.
#
# This script lives in coins/. The converter is installed in native/runtime/;
# the codec environment (.venv) stays at the PROJECT ROOT. Coin images
# live in coins/logos/{svg,png} and are written to coins/logos/emoji.

# NOT a script-wide SilentlyContinue. That swallowed every failure below,
# including a Start-Process that never started: $proc stayed $null, and
# '-not $null.HasExited' is $true, so the watchdog waited out the full PERFILE
# deadline 100 times -- 3.3 hours of nothing, 100 '(unknown)' quarantine
# entries, then exit 3 ("sources need review") on a first run that simply had no
# .venv yet. Only the reads that legitimately race the converter are silenced,
# one call at a time.
$ScriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Definition
$ProjectRoot = Split-Path -Parent $ScriptRoot
Set-Location -LiteralPath $ScriptRoot

$py       = Join-Path $ProjectRoot '.venv\Scripts\python.exe'
$convert  = Join-Path $ProjectRoot 'native\runtime\numera-emoji.exe'
$owner    = Join-Path $ProjectRoot 'scripts\native_convert_owner.py'
$svgDir   = Join-Path $ScriptRoot 'logos\svg'
$pngDir   = Join-Path $ScriptRoot 'logos\png'
$emojiDir = Join-Path $ScriptRoot 'logos\emoji'
$marker   = Join-Path $emojiDir '.svg_cur'

# This script is run directly, not through run.ps1, so the venv it needs may
# simply not exist yet. Say so in one line instead of watchdogging a process
# that was never started.
foreach ($required in @($py, $convert, $owner)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        Write-Host "[watchdog] required file not found: $required"
        Write-Host "[watchdog] create the project venv, install requirements and run scripts/build_native.py first (see README)."
        exit 2
    }
}

$exit = 0
$quarantined = @()
$finished = $false

# Each pass is owned by the selected Python wrapper's private native Job.
# Its 120s marker-idle and 3600s wall bounds stop and reap the entire tree.
for ($iter = 1; $iter -le 100; $iter++) {
    & $py $owner --in $svgDir --out $emojiDir --stdout (Join-Path $ScriptRoot 'emoji_conv.txt') --stderr (Join-Path $ScriptRoot 'emoji_err.txt')
    $code = $LASTEXITCODE
    if ($code -eq 124) {
        $stuck = "$(Get-Content -LiteralPath $marker -Raw -ErrorAction SilentlyContinue)".Trim()
        if (-not $stuck) { $stuck = '(unknown)' }
        $quarantined += $stuck
        Write-Host "[watchdog] '$stuck' made no progress for 120s (iter $iter) - killed and reaped."
        continue
    }
    if ($code -eq 125) {
        Write-Host '[watchdog] conversion exceeded its 3600s wall bound - see emoji_err.txt'
        $exit = 1
        break
    }

    # Only an owned idle-timeout kill justifies a restart. An ordinary exit
    # without DONE is an error, not permission to silently retry it.
    $tail = Get-Content 'emoji_conv.txt' -Tail 1 -ErrorAction SilentlyContinue
    if ($tail -match '^DONE:') {
        Write-Host "[watchdog] SVG conversion complete: $tail"
        if ($code -gt $exit) { $exit = $code }
        $finished = $true
    } else {
        Write-Host "[watchdog] converter exited (code $($code)) without finishing - see emoji_err.txt"
        $exit = if ($code -gt 1) { $code } else { 1 }
    }
    break
}

# Surface the converter's own quarantine notice (entries recorded by an earlier
# run) plus anything this run killed: a skipped source must never be dropped
# silently, and the run is not "clean" while one is waiting for review.
$review = Get-Content 'emoji_conv.txt' -ErrorAction SilentlyContinue |
    Select-String -Pattern '^(REVIEW|QUARANTINE):'
foreach ($line in $review) { Write-Host "[watchdog] $($line.Line)" }
if ($quarantined.Count) {
    Write-Host "[watchdog] REVIEW killed sources: $($quarantined -join ', ') - listed in $(Join-Path $emojiDir '.svg_skip.txt'); delete a line to retry one."
}
if (($review -or $quarantined.Count) -and $exit -lt 3) { $exit = 3 }

if (-not $finished) {
    if ($exit -eq 0) { $exit = 1 }
    Write-Host "[watchdog] SVG conversion did NOT complete - giving up (exit $exit)."
    exit $exit
}

# Pass 2: PNG fallback gets the same owned process-tree and timeout bounds.
if (Test-Path -LiteralPath $pngDir) {
    & $py $owner --in $pngDir --out $emojiDir --stdout (Join-Path $ScriptRoot 'emoji_png_log.txt') --stderr (Join-Path $ScriptRoot 'emoji_png_err.txt')
    $code = $LASTEXITCODE
    Get-Content -LiteralPath (Join-Path $ScriptRoot 'emoji_png_log.txt') -ErrorAction SilentlyContinue
    if ($code -in @(124, 125)) {
        Write-Host '[watchdog] PNG conversion stopped at its owned timeout - sources need review.'
        $code = 1
    }
    if ($code -gt $exit) { $exit = $code }
}

exit $exit
