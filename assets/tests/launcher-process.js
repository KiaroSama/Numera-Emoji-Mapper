// Execute the reviewed launchers with synthetic native programs, never operator binaries.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { appendFileSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, test } from 'node:test';

const project = fileURLToPath(new URL('../../', import.meta.url));
const pwsh = process.env.NUMERA_TEST_PWSH || 'pwsh';
const checkpoint = path.join(project, 'logs', 'checkpoint');
mkdirSync(checkpoint, { recursive: true });
const receipt = path.join(checkpoint, `launcher-process-${process.platform}-${process.arch}.log`);
writeFileSync(receipt, '');
const levels = new Set(['INFO', 'WARNING', 'ERROR', 'DEBUG']);
function log(level, message) {
  assert(levels.has(level));
  const line = `${new Date().toISOString()} [${level}] [launcher-test] ${message}`;
  appendFileSync(receipt, `${line}\n`);
  console.log(line);
}
const temp = path.join(project, 'logs', 'tmp');
mkdirSync(temp, { recursive: true });
const root = mkdtempSync(path.join(temp, "launcher-tests-α ; ' fixture-"));
let passed = true;
let sequence = 0;
let completed = 0;
let failures = 0;
const inherited = new Set(['path', 'pathext', 'systemroot', 'windir', 'comspec', 'temp', 'tmp',
  'home', 'userprofile', 'localappdata', 'appdata', 'programfiles', 'programfiles(x86)', 'programw6432',
  'psmodulepath', 'lang', 'lc_all', 'dotnet_root', 'rustup_home', 'cargo_home',
  'include', 'lib', 'libpath', 'vcinstalldir', 'vctoolsinstalldir', 'vsinstalldir', 'windowssdkdir', 'windowssdkversion']);
const cleanEnvironment = Object.fromEntries(Object.entries(process.env).filter(([key]) => inherited.has(key.toLowerCase())));

// The fixture records length-prefixed UTF-8 hex, preserving empty arguments exactly.
const fixture = String.raw`
use std::{env, fs, path::Path, process, time::{SystemTime, UNIX_EPOCH}};
fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let terminal = env::current_exe().unwrap().file_name().unwrap() == "wt.exe";
    let key = if terminal { "NUMERA_LAUNCHER_TERMINAL_RECEIPT" } else { "NUMERA_LAUNCHER_NATIVE_RECEIPT" };
    let target = env::var_os(key).expect("synthetic receipt path");
    let hex: Vec<String> = args.iter().map(|arg| arg.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect()).collect();
    fs::write(Path::new(&target), format!("{}\n{}", args.len(), hex.join("\n"))).unwrap();
    if !terminal {
        fs::write(Path::new(&target).with_extension("cwd"), env::current_dir().unwrap().to_string_lossy().as_bytes()).unwrap();
    }
    let code = if terminal { 0 } else { env::var("NUMERA_LAUNCHER_NATIVE_EXIT").unwrap().parse::<i32>().unwrap() };
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    if !terminal { println!("{stamp} [INFO] [synthetic-native] stdout receipt"); }
    eprintln!("{stamp} [INFO] [synthetic-native] completed with code {code}");
    process::exit(code);
}
`;
writeFileSync(path.join(root, 'fixture.rs'), fixture);
const compiler = spawnSync('rustc', ['--edition=2024', path.join(root, 'fixture.rs'), '-o', path.join(root, 'fixture.exe')], {
  cwd: project, env: cleanEnvironment, encoding: 'utf8', timeout: 60_000, windowsHide: true,
});
if (compiler.status !== 0) log('ERROR', `Synthetic fixture compilation failed: ${compiler.error?.message || compiler.stderr}`);
assert.equal(compiler.status, 0, 'Synthetic fixture compilation failed');
log('DEBUG', 'Built the argument/exit fixture; all launcher binaries are synthetic');

const decode = file => {
  const [count, ...rows] = readFileSync(file, 'utf8').split('\n');
  if (Number(count) === 0) return [];
  assert.equal(rows.length, Number(count));
  return rows.map(row => Buffer.from(row, 'hex').toString('utf8'));
};
const encoded = source => Buffer.from(source, 'utf16le').toString('base64');
const unpack = '$case = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($env:NUMERA_LAUNCHER_CASE)) | ConvertFrom-Json -AsHashtable';
const ordinaryArgs = ['status', '--help', '--output', 'two words', 'literal"quote', "single'quote", ';',
  '$(Write-Output injected)', 'سلام 😀', '', 'synthetic-secret-do-not-log'];

function setup(script, parameters = {}, code = 0, options = {}) {
  const dir = path.join(root, `case-${++sequence}`);
  mkdirSync(path.join(dir, 'bin'), { recursive: true });
  copyFileSync(path.join(project, script), path.join(dir, script));
  if (!options.missingBinary) copyFileSync(path.join(root, 'fixture.exe'), path.join(dir, 'bin', 'numera-emoji.exe'));
  copyFileSync(path.join(root, 'fixture.exe'), path.join(dir, 'wt.exe'));
  const data = { Script: path.join(dir, script), Directory: dir, Parameters: parameters,
    NativePreference: Boolean(options.nativePreference), NoRelaunch: Boolean(options.noRelaunch),
    LegacyArguments: Boolean(options.legacyArguments), PowerShellPath: options.powerShellPath || '' };
  if (options.failedLogDirectory) writeFileSync(path.join(dir, 'logs'), 'synthetic directory obstruction');
  return { dir, script, data, env: { ...cleanEnvironment,
    NUMERA_EMOJI_MAPPER_NO_DOTENV: '1',
    NUMERA_LAUNCHER_CASE: Buffer.from(JSON.stringify(data)).toString('base64'),
    NUMERA_LAUNCHER_NATIVE_RECEIPT: path.join(dir, 'native.receipt'),
    NUMERA_LAUNCHER_TERMINAL_RECEIPT: path.join(dir, 'terminal.receipt'),
    NUMERA_LAUNCHER_FAKE_TERMINAL: path.join(dir, 'wt.exe'),
    NUMERA_LAUNCHER_LOG_ATTEMPTS: path.join(dir, 'log-attempts.receipt'),
    NUMERA_LAUNCHER_NATIVE_EXIT: String(code),
  } };
}

async function invoke(context, source, args) {
  const child = spawn(pwsh, args || ['-NoLogo', '-NoProfile', '-OutputFormat', 'Text', '-EncodedCommand', encoded(source)], {
    cwd: root, env: context.env, windowsHide: true, detached: process.platform !== 'win32',
  });
  let stdout = '';
  let stderr = '';
  let timedOut = false;
  child.stdout.setEncoding('utf8').on('data', chunk => { stdout += chunk; });
  child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
  child.stdin.on('error', () => {});
  child.stdin.end('\n');
  const timer = setTimeout(() => {
    timedOut = true;
    if (process.platform === 'win32') {
      spawnSync(path.join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'taskkill.exe'),
        ['/PID', String(child.pid), '/T', '/F'], { env: cleanEnvironment, windowsHide: true, timeout: 10_000, stdio: 'ignore' });
    } else {
      try { process.kill(-child.pid, 'SIGKILL'); } catch (error) { if (error.code !== 'ESRCH') throw error; }
    }
    child.kill('SIGKILL');
  }, 20_000);
  try {
    const result = await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('close', (status, signal) => resolve({ status, signal }));
    });
    assert(!timedOut, 'PowerShell timed out; its owned process tree was terminated');
    assert.equal(result.signal, null, 'PowerShell was interrupted');
    return { ...result, stdout, stderr, output: `${stdout}\n${stderr}` };
  } finally {
    clearTimeout(timer);
  }
}

function checkLogs(context) {
  const directory = path.join(context.dir, 'logs');
  if (!existsSync(directory)) return;
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isFile()) continue;
    const text = readFileSync(path.join(directory, entry.name), 'utf8');
    assert(!text.includes('synthetic-secret-do-not-log'), 'launcher logged a command argument');
    assert(!text.includes(context.env.NUMERA_LAUNCHER_CASE), 'launcher logged a serialized argument payload');
    for (const line of text.trim().split('\n')) {
      assert.match(line, /^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} UTC\] \[(INFO|WARNING|ERROR|DEBUG)\]/);
    }
  }
}

async function direct(context, failLog = false) {
  const source = [unpack, '$PSNativeCommandUseErrorActionPreference = $case.NativePreference'];
  if (context.data.LegacyArguments) source.push('$PSNativeCommandArgumentPassing = "Legacy"');
  if (failLog) {
    // Fail the real file write on every attempt, without relying on OS-specific ACLs.
    source.push('function Add-Content { [CmdletBinding()] param([string]$LiteralPath,[string]$Encoding,[object]$Value) [IO.File]::AppendAllText($env:NUMERA_LAUNCHER_LOG_ATTEMPTS,"1"); [void][IO.Directory]::CreateDirectory($LiteralPath); [IO.File]::AppendAllText($LiteralPath,[string]$Value) }');
  }
  source.push('$parameters = $case.Parameters', '& $case.Script @parameters -NoRelaunch', 'exit $LASTEXITCODE');
  const result = await invoke(context, source.join(';\n'));
  if (existsSync(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT)) {
    assert.equal(path.resolve(readFileSync(path.join(context.dir, 'native.cwd'), 'utf8')), path.resolve(root));
  }
  return result;
}

async function captureReentry(context) {
  // Execute the real Explorer branch and its real helper definitions. Explorer
  // detection is supplied by the test; the arguments are captured by a native
  // terminal fixture and then used to launch the actual child PowerShell.
  const source = [unpack,
    '$tokens = $null; $errors = $null; $ast = [Management.Automation.Language.Parser]::ParseFile($case.Script,[ref]$tokens,[ref]$errors)',
    'if ($errors.Count) { throw "Launcher parse failed" }',
    'foreach ($statement in $ast.EndBlock.Statements) { if ($statement -is [Management.Automation.Language.FunctionDefinitionAst]) { . ([scriptblock]::Create($statement.Extent.Text)) } }',
    '$PSScriptRoot = $case.Directory; $PSCommandPath = $case.Script; $fromExplorer = $true; $NoRelaunch = $case.NoRelaunch; $env:WT_SESSION = $null',
    '$Check = $false; $All = $false; $WithPack = [int[]]@(); $CommandArguments = [string[]]@(); $ExtraArgs = [string[]]@()',
    'foreach ($item in $case.Parameters.GetEnumerator()) { Set-Variable -Name $item.Key -Value $item.Value }',
    'function Get-Command { [CmdletBinding()] param([string]$Name) if ($Name -eq "wt.exe") { [pscustomobject]@{Source=$env:NUMERA_LAUNCHER_FAKE_TERMINAL} } else { throw "Unexpected command discovery" } }',
    'if ($case.PowerShellPath) { function Get-Process { param([int]$Id) [pscustomobject]@{Path=$case.PowerShellPath} } }',
    '$branch = @($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.IfStatementAst] -and $_.Clauses[0].Item1.Extent.Text.Contains("$" + "fromExplorer") })',
    'if ($branch.Count -ne 1) { throw "Expected one Explorer relaunch branch" }',
    '. ([scriptblock]::Create(\'$PSScriptRoot = $case.Directory; $PSCommandPath = $case.Script; \' + $branch[0].Extent.Text))',
  ].join(';\n');
  const launcher = await invoke(context, source);
  assert.equal(launcher.status, 0, launcher.output);
  if (context.data.NoRelaunch) {
    assert(!existsSync(context.env.NUMERA_LAUNCHER_TERMINAL_RECEIPT));
    return launcher;
  }
  assert(existsSync(context.env.NUMERA_LAUNCHER_TERMINAL_RECEIPT), 'terminal was not started');
  return decode(context.env.NUMERA_LAUNCHER_TERMINAL_RECEIPT);
}

function terminalCommand(args) {
  // This tests WT's documented delimiter boundary, not its GUI or complete parser.
  // Upstream: AppCommandlineArgs::_commandDelimiterRegex and Commandline::AddArg.
  assert(args.every(arg => !/(^|[^\\]);/.test(arg)), 'WT would split a literal semicolon into another command');
  return args.map(arg => arg.replaceAll('\\;', ';'));
}

async function reentry(context) {
  const captured = await captureReentry(context);
  if (context.data.NoRelaunch) return captured;
  const args = terminalCommand(captured);
  const start = args.indexOf('-NoLogo');
  assert(start > 0, 'terminal did not receive a PowerShell command');
  assert.equal(path.resolve(args[start - 1]), path.resolve(pwsh.includes(path.sep) ? pwsh : args[start - 1]));
  const childArguments = args.slice(start);
  if (context.data.LegacyArguments) {
    const offset = childArguments.indexOf('-EncodedCommand');
    assert(offset >= 0, 'reentry must encode typed script parameters');
    const source = Buffer.from(childArguments[offset + 1], 'base64').toString('utf16le');
    childArguments[offset + 1] = encoded('$PSNativeCommandArgumentPassing = "Legacy";\n' + source);
  }
  const child = await invoke(context, '', childArguments);
  if (existsSync(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT)) {
    assert.equal(path.resolve(readFileSync(path.join(context.dir, 'native.cwd'), 'utf8')), path.resolve(context.dir));
  }
  checkLogs(context);
  return child;
}

function scenario(name, run) {
  test(name, async () => {
    try { await run(); completed++; log('INFO', `PASS ${name}`); }
    catch (error) { passed = false; failures++; log('ERROR', `FAIL ${name}`); throw error; }
  });
}

for (const [label, script, parameters, expected] of [
  ['menu', 'run.ps1', {}, ['menu']],
  ['doctor', 'run.ps1', { Check: true }, ['doctor']],
  ['literal passthrough', 'run.ps1', { CommandArguments: ordinaryArgs }, ordinaryArgs],
  ['default panel', 'run-panel.ps1', {}, ['panel']],
  ['single pack', 'run-panel.ps1', { WithPack: [6] }, ['panel', '--with-pack', '6']],
  ['multiple packs and passthrough', 'run-panel.ps1', { WithPack: [6, 7, 8], All: true, ExtraArgs: ordinaryArgs },
    ['panel', '--with-pack', '6', '--with-pack', '7', '--with-pack', '8', '--all', ...ordinaryArgs]],
]) {
  scenario(`${script} direct ${label}`, async () => {
    const context = setup(script, parameters);
    const result = await direct(context);
    assert.equal(result.status, 0, result.output);
    assert.deepEqual(decode(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT), expected);
    checkLogs(context);
  });
}

for (const script of ['run.ps1', 'run-panel.ps1']) {
  scenario(`${script} preserves PowerShell output capture and caller preferences`, async () => {
    const context = setup(script, {}, 3);
    const source = [unpack,
      '$PSNativeCommandArgumentPassing = "Legacy"; $PSNativeCommandUseErrorActionPreference = $true',
      '$parameters = $case.Parameters; $captured = @(& $case.Script @parameters -NoRelaunch); $code = $LASTEXITCODE',
      'if ($PSNativeCommandArgumentPassing -ne "Legacy" -or -not $PSNativeCommandUseErrorActionPreference) { throw "Launcher changed caller preferences" }',
      '[IO.File]::WriteAllText((Join-Path $case.Directory "captured.receipt"), ($captured -join "`n"))',
      'exit $code',
    ].join(';\n');
    const result = await invoke(context, source);
    assert.equal(result.status, 3, result.output);
    assert.match(readFileSync(path.join(context.dir, 'captured.receipt'), 'utf8'), /^\d+ \[INFO\] \[synthetic-native\] stdout receipt$/);
    assert(!result.stdout.includes('[synthetic-native] stdout receipt'), 'native output escaped the PowerShell capture');
  });

  scenario(`${script} preserves literal native arguments under Legacy passing`, async () => {
    const parameters = script === 'run.ps1' ? { CommandArguments: ordinaryArgs } : { ExtraArgs: ordinaryArgs };
    const context = setup(script, parameters, 0, { legacyArguments: true });
    const result = await direct(context);
    assert.equal(result.status, 0, result.output);
    assert.deepEqual(decode(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT), script === 'run.ps1' ? ordinaryArgs : ['panel', ...ordinaryArgs]);
    checkLogs(context);
  });

  scenario(`${script} forwards option-looking native arguments from -File`, async () => {
    const context = setup(script);
    const args = script === 'run.ps1' ? ['status', '--help', '--output', 'two words'] : ['--help', '--bind', '127.0.0.1'];
    const result = await invoke(context, '', ['-NoLogo', '-NoProfile', '-File', context.data.Script, '-NoRelaunch', ...args]);
    assert.equal(result.status, 0, result.output);
    assert.deepEqual(decode(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT), script === 'run.ps1' ? args : ['panel', ...args]);
  });

  scenario(`${script} rejects invalid input before invoking native code`, async () => {
    const parameters = script === 'run.ps1' ? { Check: true, CommandArguments: ['status'] } : { WithPack: [0, 6] };
    const context = setup(script, parameters);
    assert.equal((await direct(context)).status, 1);
    assert(!existsSync(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT));
  });

  scenario(`${script} reports a missing native binary`, async () => {
    const context = setup(script, {}, 0, { missingBinary: true });
    const result = await direct(context);
    assert.equal(result.status, 1);
    assert.match(result.output, /Native executable not found/);
  });

  for (const code of [0, 3, 7]) {
    scenario(`${script} preserves native exit ${code} with native-error preference enabled`, async () => {
      const context = setup(script, {}, code, { nativePreference: true });
      const result = await direct(context);
      assert.equal(result.status, code, result.output);
      assert(existsSync(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT));
    });
    scenario(`${script} preserves native exit ${code} after one failed log write`, async () => {
      const context = setup(script, {}, code, { nativePreference: true });
      const result = await direct(context, true);
      assert.equal(result.status, code, result.output);
      assert(existsSync(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT));
      assert.equal(readFileSync(context.env.NUMERA_LAUNCHER_LOG_ATTEMPTS, 'utf8'), '1');
      assert.equal((result.stdout.match(/\[WARNING\].*logging disabled/gi) || []).length, 1);
    });
  }

  scenario(`${script} relaunch loop guard prevents a terminal start`, async () => {
    await reentry(setup(script, {}, 0, { noRelaunch: true }));
  });

  scenario(`${script} continues after log directory creation fails`, async () => {
    const context = setup(script, {}, 7, { failedLogDirectory: true });
    const result = await direct(context);
    assert.equal(result.status, 7, result.output);
    assert(existsSync(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT));
    assert.equal((result.stdout.match(/\[WARNING\].*logging disabled/gi) || []).length, 1);
  });

  scenario(`${script} WT boundary preserves a semicolon PowerShell path`, async () => {
    // A directory separator followed by ';' must retain that separator, too.
    const powerShellPath = "C:\\PowerShell α; folder\\;sub 'folder\\pwsh.exe";
    const captured = await captureReentry(setup(script, {}, 0, { powerShellPath }));
    assert.throws(() => terminalCommand(['pwsh.exe', 'literal;argument']), /WT would split/);
    const args = terminalCommand(captured);
    assert.equal(args[args.indexOf('-NoLogo') - 1], powerShellPath);
    assert(!args.includes('-d'), 'script directory belongs in decoded data, outside WT parsing');
  });

  scenario(`${script} real child reentry preserves literal arguments under Legacy passing`, async () => {
    const parameters = script === 'run.ps1' ? { CommandArguments: ordinaryArgs } : { WithPack: [6, 7], ExtraArgs: ordinaryArgs };
    const context = setup(script, parameters, 0, { legacyArguments: true });
    const result = await reentry(context);
    assert.equal(result.status, 0, result.output);
    assert.deepEqual(decode(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT), script === 'run.ps1' ? ordinaryArgs : ['panel', '--with-pack', '6', '--with-pack', '7', ...ordinaryArgs]);
  });

  scenario(`${script} error pause preserves the native exit`, async () => {
    const context = setup(script, { PauseOnError: true }, 7);
    const result = await direct(context);
    assert.equal(result.status, 7, result.output);
    assert.match(result.output, /Press Enter to close/);
  });

  for (const [packs, code] of [[[6], 0], [[6, 7], 3], [[6, 7, 8], 7]]) {
    scenario(`${script} real child reentry preserves ${packs.length} pack(s), literal arguments, and exit ${code}`, async () => {
      const parameters = script === 'run.ps1' ? { CommandArguments: ordinaryArgs } : { WithPack: packs, All: true, ExtraArgs: ordinaryArgs };
      const context = setup(script, parameters, code);
      const result = await reentry(context);
      assert.equal(result.status, code, result.output);
      const expected = script === 'run.ps1' ? ordinaryArgs : ['panel', ...packs.flatMap(pack => ['--with-pack', String(pack)]), '--all', ...ordinaryArgs];
      assert.deepEqual(decode(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT), expected);
      assert.equal((result.stdout.match(/Press Enter to close/g) || []).length, code ? 1 : 0);
    });
  }
}

after(() => {
  log(passed ? 'INFO' : 'ERROR', `Completed ${completed + failures} cases: ${completed} passed, ${failures} failed; no skips`);
  if (passed) {
    rmSync(root, { recursive: true, force: true });
    log('DEBUG', 'Removed owned launcher fixtures after successful checks');
  } else {
    log('WARNING', `Retained failing synthetic launcher fixtures at ${root}`);
  }
});
