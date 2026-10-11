# Native launcher boundary. Paths and arguments remain data; never compile on a menu action.
function Invoke-Native ($py, [string[]]$Argv) {
    $launcher = Join-Path $ScriptRoot 'scripts\launch_prebuilt.py'
    if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) {
        Write-Err 'Native launcher missing; install a verified package.'
        return 2
    }
    Invoke-Py $py (@($launcher) + $Argv)
}

function Invoke-NativeReport ($py, [string[]]$Argv, $what) {
    $code = Invoke-Native $py $Argv
    if ($code -eq 0) { Write-Ok "$what finished." }
    elseif ($code -eq 3) { Write-Warn "$what partially finished; review the log before retrying." }
    else { Write-Err "$what failed (exit $code)." }
}

function Test-NativeRuntime ($py) {
    $binary = Join-Path $ScriptRoot 'native\runtime\numera-emoji.exe'
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        Write-Err 'Installed native executable missing; install a verified prebuilt package.'
        return $false
    }
    $result = & $py (Join-Path $ScriptRoot 'scripts\launch_prebuilt.py') --help 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0 -or $result -notmatch 'Numera Emoji Mapper native backend') {
        Write-Err 'Installed native executable failed its startup probe.'
        return $false
    }
    return $true
}
