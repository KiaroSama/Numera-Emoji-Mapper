# The launcher's menu actions, dot-sourced by run.ps1 (not run on its own).
#
# Every input prompt supports {back=0, quit=exit}: 0 aborts to the menu, exit
# quits. Each Python launch is logged (command + exit code) via Invoke-Py.
# Uses run.ps1's helpers (Ask, Ask-YesNo, Invoke-Py, Write-*) and $ScriptRoot.

# 'mixed', 'per-format', 'new' (no sets yet) or 'ambiguous' for a pack family,
# read from the publisher's own state file. A family started one way refuses
# to continue the other, so the launcher must not guess.
function Get-FamilyMode ([string]$base) {
    $state = Join-Path $ScriptRoot "collection\publish_$base.json"
    if (-not (Test-Path -LiteralPath $state)) { return 'new' }
    try { $sets = @((Get-Content -LiteralPath $state -Raw -Encoding UTF8 | ConvertFrom-Json).sets) }
    catch { return 'ambiguous' }
    if ($sets.Count -eq 0) { return 'new' }
    $mixed = @($sets | Where-Object { $_.fmt -eq 'mixed' }).Count
    if ($mixed -eq $sets.Count) { return 'mixed' }
    if ($mixed -eq 0) { return 'per-format' }
    return 'ambiguous'
}

function Action-BuildGeneral ($py) {
    Write-Title "Build a general emoji pack (@YourEmojiBot)"
    $st = @{ emoji = '😀' }
    $steps = @(
        { $v = Ask "Source image folder (e.g. input\myset)"; if ($v -eq '0') { return 'back' }
          if ([string]::IsNullOrWhiteSpace($v) -or -not (Test-Path -LiteralPath $v)) {
              Write-Err "Folder not found: $v"; return 'stay' }
          $st.inDir = $v; 'ok' }.GetNewClosure(),
        { $v = Ask "Pack base name (letters/digits/_), e.g. myset"; if ($v -eq '0') { return 'back' }
          if ([string]::IsNullOrWhiteSpace($v)) { Write-Err "Base name required."; return 'stay' }
          $st.base = $v; 'ok' }.GetNewClosure(),
        { $v = Ask "Pack title, e.g. My Emojis"; if ($v -eq '0') { return 'back' }
          if ([string]::IsNullOrWhiteSpace($v)) { Write-Err "Title required."; return 'stay' }
          $st.title = $v; 'ok' }.GetNewClosure(),
        { $v = Ask "Associated standard emoji (default 😀)"; if ($v -eq '0') { return 'back' }
          if (-not [string]::IsNullOrWhiteSpace($v)) { $st.emoji = $v }; 'ok' }.GetNewClosure(),
        { $yn = Ask-YesNo "Convert + dry-run + upload now?"; if ($yn -is [string]) { return 'back' }
          if (-not $yn) { Write-Info "Cancelled."; return 'ok' }
          $build = Join-Path 'build' $st.base
          Write-Step "Converting images -> $build ..."
          if ((Invoke-Py $py @('-m','emojikit.make_emoji_pngs','--in',$st.inDir,'--out',$build)) -ne 0) {
              Write-Err "Conversion failed."; return 'ok' }
          Write-Step "Dry-run preview ..."
          if ((Invoke-Py $py @('-m','emojikit.build_pack','--base',$st.base,'--title',$st.title,'--source-dir',$build,
                               '--token-env','GENERAL_BOT_TOKEN','--emoji',$st.emoji,'--dry-run')) -ne 0) {
              Write-Err "Dry-run failed (check .env / source)."; return 'ok' }
          if ((Invoke-Py $py @('-m','emojikit.build_pack','--base',$st.base,'--title',$st.title,'--source-dir',$build,
                               '--token-env','GENERAL_BOT_TOKEN','--emoji',$st.emoji)) -eq 0) {
              Write-Ok "Pack build finished." } else { Write-Err "Build failed." }
          'ok' }.GetNewClosure()
    )
    Run-Wizard $steps | Out-Null
}

function Action-ConvertOnly ($py) {
    Write-Title "Convert images to 100x100 PNGs"
    $st = @{}
    $steps = @(
        { $v = Ask "Source image folder"; if ($v -eq '0') { return 'back' }
          if (-not (Test-Path -LiteralPath $v)) { Write-Err "Folder not found."; return 'stay' }
          $st.inDir = $v; 'ok' }.GetNewClosure(),
        { $v = Ask "Output folder (blank = <folder>_emoji)"; if ($v -eq '0') { return 'back' }
          $st.outDir = $v
          $argv = @('-m','emojikit.make_emoji_pngs','--in',$st.inDir)
          if (-not [string]::IsNullOrWhiteSpace($st.outDir)) { $argv += @('--out',$st.outDir) }
          Invoke-PyReport $py $argv "Conversion"
          'ok' }.GetNewClosure()
    )
    Run-Wizard $steps | Out-Null
}

function Action-CoinRebuild ($py) {
    Write-Title "Crypto-coin pack rebuild (TELEGRAM_BOT_TOKEN)"
    Write-Warn "This uses the crypto-coin bot and the coins/ component."
    $script = Join-Path $ScriptRoot 'coins\rebuild_dedup.py'
    if (-not (Test-Path -LiteralPath $script)) { Write-Err "coins\rebuild_dedup.py not found."; return }
    # The default command `all` first DELETES every pack recorded in the legacy
    # coins\rebuild_state.json. Enter-means-yes made that the last guard before
    # a whole family was deleted, and the old question never said so. Only the
    # COUNT is read and shown, never the pack names.
    $oldState = Join-Path $ScriptRoot 'coins\rebuild_state.json'
    if (Test-Path -LiteralPath $oldState) {
        $n = '?'
        try { $n = @((Get-Content -LiteralPath $oldState -Raw -Encoding utf8 | ConvertFrom-Json).sets).Count } catch { }
        $yn = Ask-YesNoDefaultNo "This DELETES the $n pack(s) recorded in coins\rebuild_state.json, then rebuilds them. Continue?"
    } else {
        $yn = Ask-YesNo "Run coins/rebuild_dedup.py now? (duplicate-proof: build + map + links)"
    }
    if ($yn -is [string] -or -not $yn) { return }   # back or no -> return to menu
    Invoke-PyReport $py @('-m', 'coins.rebuild_dedup') "Coin pack rebuild"   # run.ps1 works from the project root
}

function Action-CollectPacks ($py) {
    Write-Title "Collect emoji from existing Telegram packs"
    Write-Info "Paste pack links/names (t.me/addemoji/...). Blank line to finish; 0 removes the last one."
    $st = @{ packs = @() }
    $steps = @(
        { $line = Ask "Pack (blank = done)"
          if ($line -eq '0') {
              # Drop the last entry. 0..(Count-2) is wrong for a single entry:
              # 0..-1 counts down and yields indices 0 and -1, i.e. that one entry twice.
              if ($st.packs.Count -gt 0) { $st.packs = @($st.packs | Select-Object -SkipLast 1); Write-Info "Removed last." }
              return 'stay' }                       # 0 = undo last entry (one step)
          if ([string]::IsNullOrWhiteSpace($line)) {
              if ($st.packs.Count -eq 0) { Write-Warn "No packs entered."; return 'back' }
              return 'ok' }
          $st.packs += $line.Trim(); return 'stay' }.GetNewClosure(),
        { $v = Ask "Token env var (default GENERAL_BOT_TOKEN)"; if ($v -eq '0') { return 'back' }
          $tokenEnv = if ([string]::IsNullOrWhiteSpace($v)) { 'GENERAL_BOT_TOKEN' } else { $v }
          Invoke-PyReport $py (@('-m','emojikit.fetch_pack') + $st.packs + @('--token-env',$tokenEnv)) "Collect"
          'ok' }.GetNewClosure()
    )
    Run-Wizard $steps | Out-Null
}

function Action-AddMedia ($py) {
    Write-Title "Build emoji from scratch (folder of images/animations/videos)"
    if (-not (Test-Ffmpeg)) {
        Write-Warn "ffmpeg/ffprobe not found: video emoji (.webm) will fail."
        Write-Warn "Install with: winget install Gyan.FFmpeg"
    }
    $st = @{ emoji = '😀' }
    $steps = @(
        { $v = Ask "Source folder"; if ($v -eq '0') { return 'back' }
          if (-not (Test-Path -LiteralPath $v)) { Write-Err "Folder not found."; return 'stay' }
          $st.inDir = $v; 'ok' }.GetNewClosure(),
        { $v = Ask "Associated standard emoji (default 😀)"; if ($v -eq '0') { return 'back' }
          if (-not [string]::IsNullOrWhiteSpace($v)) { $st.emoji = $v }
          Invoke-PyReport $py @('-m','emojikit.add_media','--in',$st.inDir,'--emoji',$st.emoji) "Add media"
          'ok' }.GetNewClosure()
    )
    Run-Wizard $steps | Out-Null
}

function Action-PublishCollection ($py) {
    Write-Title "Publish the collection into new packs (multi-format)"
    $st = @{}
    $steps = @(
        { $v = Ask "Pack base name (letters/digits only), e.g. mypack"; if ($v -eq '0') { return 'back' }
          if ([string]::IsNullOrWhiteSpace($v)) { Write-Err "Base name required."; return 'stay' }
          $st.base = $v; 'ok' }.GetNewClosure(),
        { $v = Ask "Pack title, e.g. My Collection"; if ($v -eq '0') { return 'back' }
          if ([string]::IsNullOrWhiteSpace($v)) { Write-Err "Title required."; return 'stay' }
          $st.title = $v; 'ok' }.GetNewClosure(),
        { $v = Ask "Token env var (default GENERAL_BOT_TOKEN)"; if ($v -eq '0') { return 'back' }
          $st.tokenEnv = if ([string]::IsNullOrWhiteSpace($v)) { 'GENERAL_BOT_TOKEN' } else { $v }; 'ok' }.GetNewClosure(),
        { # A family cannot switch mode once started, so an existing one keeps
          # its own; a new one is asked, with the mixed family recommended.
          $mode = Get-FamilyMode $st.base
          if ($mode -eq 'ambiguous') {
              Write-Err "collection\publish_$($st.base).json records both mixed and per-format sets; run build_collection by hand."
              return 'back' }
          if ($mode -eq 'new') {
              $yn = Ask-YesNo "Publish as ONE mixed family (recommended)?"; if ($yn -is [string]) { return 'back' }
              $mode = if ($yn) { 'mixed' } else { 'per-format' } }
          else { Write-Info "Existing family: continuing it $mode." }
          $st.mode = if ($mode -eq 'mixed') { @('--mixed') } else { @() }; 'ok' }.GetNewClosure(),
        { $yn = Ask-YesNo "Dry-run then upload now?"; if ($yn -is [string]) { return 'back' }
          if (-not $yn) { Write-Info "Cancelled."; return 'ok' }
          Write-Step "Dry-run preview ..."
          # The exit code, not a guess: a usage error, a missing operator key or
          # a busy lock all failed here, and "run a collect step first?" pointed
          # away from every one of them.
          $code = Invoke-Py $py (@('-m','emojikit.build_collection','--base',$st.base,'--title',$st.title,
                               '--token-env',$st.tokenEnv,'--dry-run') + $st.mode)
          if ($code -ne 0) {
              Write-Err "Dry-run failed (exit $code) - see the error above; exit 2 is a usage or configuration problem."; return 'ok' }
          # Ask Telegram about every queued file BEFORE the upload starts. A
          # single file it refuses used to surface at whatever minute of a
          # 45-minute publish it happened to reach; this finds it in about one.
          Write-Step "Preflight: asking Telegram to validate every queued file ..."
          if ((Invoke-Py $py (@('-m','emojikit.build_collection','--base',$st.base,'--title',$st.title,
                               '--token-env',$st.tokenEnv,'--preflight') + $st.mode)) -ne 0) {
              Write-Err "Preflight refused a file. Nothing was published."; return 'ok' }
          if ((Invoke-Py $py (@('-m','emojikit.build_collection','--base',$st.base,'--title',$st.title,
                               '--token-env',$st.tokenEnv) + $st.mode)) -eq 0) {
              Write-Ok "Collection published." } else { Write-Err "Publish failed." }
          'ok' }.GetNewClosure()
    )
    Run-Wizard $steps | Out-Null
}

function Action-Panel ($py) {
    Write-Title "Curate panel (pick which emoji go into the pack)"
    # The panel hides emoji that are already live, so its default view shows the
    # NEXT pack's candidates and nothing else. Arranging a published pack needs
    # --with-pack per set, and opening the menu entry without it looked like the
    # packs had vanished. Read the sets from the publisher's own state file.
    $packs = @()
    foreach ($state in (Get-ChildItem -LiteralPath (Join-Path $ScriptRoot 'collection') `
                        -Filter 'publish_*.json' -File -ErrorAction SilentlyContinue)) {
        try {
            $doc = Get-Content -LiteralPath $state.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
            foreach ($set in $doc.sets) { if ($null -ne $set.index) { $packs += [int]$set.index } }
        } catch { }
    }
    $packs = $packs | Sort-Object -Unique
    $panelArgs = @('-m','emojikit.panel')
    if ($packs.Count -gt 0) {
        $answer = Ask-YesNo ("Also show the " + $packs.Count + " pack(s) already published, so they can be rearranged?")
        # NOTE: compare by type, not -eq 'back' -- Ask-YesNo can return a bare
        # [bool], and PowerShell's -eq coerces a string operand to match a bool
        # LHS ('back' -> $true), so `$true -eq 'back'` is True. That made every
        # "yes" answer here read as "back": the panel silently returned to the
        # menu, and a "no" answer opened it without --with-pack (unexplained
        # single-emoji view). Confirmed empirically; same fix applied to the
        # three other Ask-YesNo call sites in this file.
        if ($answer -is [string]) { return }
        if ($answer) { foreach ($n in $packs) { $panelArgs += @('--with-pack', "$n") } }
    }
    Write-Info "Opening the curation panel in your browser..."
    Invoke-PyReport $py $panelArgs "Web panel"
}

function Action-RunBot ($py) {
    Write-Title "Run the Numera Emoji Mapper bot (premium-emoji ID extractor)"
    Write-Info "Send the bot a premium emoji or a post with emoji, or add it to a channel/group."
    Write-Info "Press Ctrl+C to stop the bot."
    Invoke-PyReport $py @('-m','emojikit.emoji_bot') "Bot"
}

function Action-Check ($py) {
    Write-Title "Run the project checks (byte-compile + unit tests)"
    $script = Join-Path $ScriptRoot 'scripts\check.ps1'
    if (-not (Test-Path -LiteralPath $script)) { Write-Err "scripts\check.ps1 not found."; return }
    # Called with & so its `exit` ends the script, not the launcher, and its exit
    # code lands in $LASTEXITCODE. -Python pins the interpreter the launcher
    # already resolved, so the menu never checks a different environment than the
    # one its other actions use.
    Write-Log 'INFO' 'run: scripts\check.ps1'
    & $script -Python $py
    $code = [int]$LASTEXITCODE
    Write-Log 'INFO' "exit $code (scripts\check.ps1)"
    if ($code -eq 0) { Write-Ok "Project checks passed." }
    else { Write-Err "Project checks failed (exit $code)." }
}

function Action-CollectIds ($py) {
    Write-Title "Collect specific emoji by id (the GUIDE's recommended start)"
    Write-Info "Paste emoji ids (or premium-id:<id>). Blank line to finish; 0 removes the last one."
    $st = @{ ids = @() }
    $steps = @(
        { $line = Ask "Emoji id (blank = done)"
          if ($line -eq '0') {
              if ($st.ids.Count -gt 0) { $st.ids = @($st.ids | Select-Object -SkipLast 1); Write-Info "Removed last." }
              return 'stay' }
          if ([string]::IsNullOrWhiteSpace($line)) {
              if ($st.ids.Count -eq 0) { Write-Warn "No ids entered."; return 'back' }
              return 'ok' }
          $st.ids += $line.Trim(); return 'stay' }.GetNewClosure(),
        { $argv = @('-m','emojikit.fetch_emoji_ids')
          foreach ($id in $st.ids) { $argv += @('--id', $id) }
          Invoke-PyReport $py $argv "Collect by id"
          'ok' }.GetNewClosure()
    )
    Run-Wizard $steps | Out-Null
}

function Action-ReorderPack ($py) {
    Write-Title "Reorder a live pack to match the panel (ids survive)"
    $base = Ask "Pack base name"
    if ($base -eq '0' -or [string]::IsNullOrWhiteSpace($base)) { return }
    $n = Ask "Pack number (blank = every pack)"
    if ($n -eq '0') { return }
    $argv = @('-m','emojikit.sync_order','--base',$base)
    if (-not [string]::IsNullOrWhiteSpace($n)) { $argv += @('--pack', $n.Trim()) }
    Write-Step "Report only: what would move ..."
    if ((Invoke-Py $py $argv) -ne 0) { Write-Err "The report failed; nothing was moved."; return }
    $yn = Ask-YesNoDefaultNo "Apply these moves to the live pack?"
    if ($yn -is [string] -or -not $yn) { Write-Info "Nothing was moved."; return }
    Invoke-PyReport $py ($argv + @('--apply')) "Reorder"
}

function Action-Roster ($py) {
    Write-Title "Pack roster (packs/): check, then refresh if stale"
    $code = Invoke-Py $py @('-m','emojikit.pack_manifest','--check')
    if ($code -eq 0) { Write-Ok "The roster is current."; return }
    $yn = Ask-YesNo "Refresh it now (reads the live packs)?"
    if ($yn -is [string] -or -not $yn) { return }
    Invoke-PyReport $py @('-m','emojikit.pack_manifest','--refresh') "Roster refresh"
}

function Action-Archive ($py) {
    Write-Title "Pack archive: check, then sync if stale"
    $code = Invoke-Py $py @('-m','emojikit.pack_archive','--check')
    if ($code -eq 0) { Write-Ok "The archive is current."; return }
    $yn = Ask-YesNoDefaultNo "Sync it now (moves every FULL pack's media to the archive)?"
    if ($yn -is [string] -or -not $yn) { return }
    Invoke-PyReport $py @('-m','emojikit.pack_archive','--sync') "Archive sync"
}

function Action-Status ($py) {
    Write-Title "Status: is everything current? (offline, read-only)"
    Invoke-PyReport $py @('-m','emojikit.status') "Status"
}
