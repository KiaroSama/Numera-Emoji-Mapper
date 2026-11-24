// Exercise the source-checkout launcher without invoking the operator's executable.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { appendFileSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, test } from 'node:test';

const project = fileURLToPath(new URL('../../', import.meta.url));
const launcher = path.join(project, 'run.ps1');
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
    let target = env::var_os("NUMERA_LAUNCHER_NATIVE_RECEIPT").expect("synthetic receipt path");
    let hex: Vec<String> = args.iter().map(|arg| arg.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect()).collect();
    fs::write(Path::new(&target), format!("{}\n{}", args.len(), hex.join("\n"))).unwrap();
    fs::write(Path::new(&target).with_extension("cwd"), env::current_dir().unwrap().to_string_lossy().as_bytes()).unwrap();
    let code = env::var("NUMERA_LAUNCHER_NATIVE_EXIT").unwrap().parse::<i32>().unwrap();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    println!("{stamp} [INFO] [synthetic-native] stdout receipt");
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
const unpack = [
  '$case = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($env:NUMERA_LAUNCHER_CASE)) | ConvertFrom-Json -AsHashtable',
  // PowerShell 7.3 can infer dates while reading JSON; argv values must stay strings.
  'if ($case.Parameters.ContainsKey("CommandArguments")) { $case.Parameters.CommandArguments = @($case.Parameters.CommandArguments | ForEach-Object { [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($_)) }) }',
].join(';\n');
const ordinaryArgs = ['status', '--help', '--output', 'two words', 'literal"quote', "single'quote", ';',
  '$(Write-Output injected)', 'سلام 😀', '', 'synthetic-secret-do-not-log'];
// The redirected Windows host can omit the prompt; record it and still read stdin.
const recordPause = [
  'function Read-Host {',
  '  param([string]$Prompt)',
  '  [IO.File]::AppendAllText($env:NUMERA_LAUNCHER_PAUSE_RECEIPT, $Prompt + [char]10)',
  '  Microsoft.PowerShell.Utility\\Read-Host -Prompt $Prompt',
  '}',
].join('\n');
const pauses = context => existsSync(context.env.NUMERA_LAUNCHER_PAUSE_RECEIPT)
  ? readFileSync(context.env.NUMERA_LAUNCHER_PAUSE_RECEIPT, 'utf8').trimEnd().split('\n') : [];

function setup(parameters = {}, code = 0, options = {}) {
  const dir = path.join(root, `case-${++sequence}`);
  const script = path.join(dir, 'run.ps1');
  mkdirSync(path.join(dir, 'bin'), { recursive: true });
  if (options.console) {
    // Supply console observations for the headless process without adding a launcher switch.
    let source = readFileSync(launcher, 'utf8');
    for (const stream of ['Input', 'Output', 'Error']) {
      const read = `[Console]::Is${stream}Redirected`;
      assert.equal(source.split(read).length, 2, `expected one ${stream} redirection check`);
      source = source.replace(read, options.redirected === stream ? '$true' : '$false');
    }
    writeFileSync(script, source, 'utf8');
  } else {
    copyFileSync(launcher, script);
  }
  if (!options.missingBinary) copyFileSync(path.join(root, 'fixture.exe'), path.join(dir, 'bin', 'numera-emoji.exe'));
  if (options.failedLogDirectory) writeFileSync(path.join(dir, 'logs'), 'synthetic directory obstruction');
  const data = { Script: script, Directory: dir, Parameters: { ...parameters },
    NativePreference: Boolean(options.nativePreference), Parent: options.parent };
  if (parameters.CommandArguments) {
    data.Parameters.CommandArguments = parameters.CommandArguments.map(argument => Buffer.from(argument, 'utf8').toString('base64'));
  }
  return { dir, data, env: { ...cleanEnvironment,
    NUMERA_EMOJI_MAPPER_NO_DOTENV: '1',
    NUMERA_LAUNCHER_CASE: Buffer.from(JSON.stringify(data)).toString('base64'),
    NUMERA_LAUNCHER_NATIVE_RECEIPT: path.join(dir, 'native.receipt'),
    NUMERA_LAUNCHER_LOG_ATTEMPTS: path.join(dir, 'log-attempts.receipt'),
    NUMERA_LAUNCHER_PAUSE_RECEIPT: path.join(dir, 'pause.receipt'),
    NUMERA_LAUNCHER_NATIVE_EXIT: String(code),
  } };
}

async function invoke(context, source, args) {
  const childArguments = args ?? ['-NoLogo', '-NoProfile', '-OutputFormat', 'Text', '-EncodedCommand', encoded(`${recordPause}\n${source}`)];
  const child = spawn(pwsh, childArguments, {
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

const timestampedWarning = /^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} UTC\] \[WARNING\] \[run\] Launcher logging disabled; continuing without a log file\.$/gm;
function checkLogs(context) {
  const directory = path.join(context.dir, 'logs');
  const files = readdirSync(directory, { withFileTypes: true });
  assert.equal(files.length, 1);
  assert(files[0].isFile());
  assert.match(files[0].name, /^run_\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}_UTC_\d+\.log$/);
  const text = readFileSync(path.join(directory, files[0].name), 'utf8');
  assert(!text.includes('synthetic-secret-do-not-log'), 'launcher logged a command argument');
  assert(!text.includes(context.env.NUMERA_LAUNCHER_CASE), 'launcher logged a serialized argument payload');
  for (const line of text.trim().split('\n')) {
    assert.match(line, /^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} UTC\] \[(INFO|WARNING|ERROR|DEBUG)\]/);
  }
  assert.match(text, /\[INFO\] \[run\] exit code=0/);
}

async function direct(context, { failLog = false, legacyArguments = false } = {}) {
  const source = [unpack, '$PSNativeCommandUseErrorActionPreference = $case.NativePreference'];
  if (legacyArguments) source.push('$PSNativeCommandArgumentPassing = "Legacy"');
  if (Object.hasOwn(context.data, 'Parent') && context.data.Parent !== undefined) {
    source.push('function Get-Process { [CmdletBinding()] param([int]$Id) [pscustomobject]@{ Parent = if ($null -ne $case.Parent) { [pscustomobject]@{ProcessName=$case.Parent} } } }');
  }
  if (failLog) {
    // Block the actual file write without relying on OS-specific ACLs.
    source.push('function Add-Content { [CmdletBinding()] param([string]$LiteralPath,[string]$Encoding,[object]$Value) [IO.File]::AppendAllText($env:NUMERA_LAUNCHER_LOG_ATTEMPTS,"1"); [void][IO.Directory]::CreateDirectory($LiteralPath); [IO.File]::AppendAllText($LiteralPath,[string]$Value) }');
  }
  source.push('$parameters = $case.Parameters', '& $case.Script @parameters', 'exit $LASTEXITCODE');
  const result = await invoke(context, source.join(';\n'));
  if (existsSync(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT)) {
    assert.equal(path.resolve(readFileSync(path.join(context.dir, 'native.cwd'), 'utf8')), path.resolve(root));
  }
  return result;
}

function scenario(name, run) {
  test(name, async () => {
    try { await run(); completed++; log('INFO', `PASS ${name}`); }
    catch (error) { failures++; log('ERROR', `FAIL ${name}`); throw error; }
  });
}

scenario('the checkout has one launcher and does not start another terminal', () => {
  assert(!existsSync(path.join(project, 'run-panel.ps1')), 'remove the second launcher');
  assert.doesNotMatch(readFileSync(launcher, 'utf8'), /wt\.exe|-NoRelaunch|-PauseOnError|-EncodedCommand/i);
});

for (const [label, parameters, expected] of [
  ['menu', {}, ['menu']],
  ['doctor', { Check: true }, ['doctor']],
  ['panel', { CommandArguments: ['panel'] }, ['panel']],
  ['literal passthrough', { CommandArguments: ordinaryArgs }, ordinaryArgs],
  ['timestamp-like text', { CommandArguments: ['2026-11-24T12:34:56Z', '2026-11-24T12:34:56.1234567Z',
    '2026-11-24T12:34:56+03:30', '2026-11-24T12:34:56', '2026-11-24', 'true', 'null', '0', '', 'سلام 😀'] }, null],
]) {
  scenario(`run.ps1 forwards ${label}`, async () => {
    const context = setup(parameters);
    const result = await direct(context);
    assert.equal(result.status, 0, result.output);
    assert.deepEqual(decode(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT), expected ?? parameters.CommandArguments);
    checkLogs(context);
  });
}

scenario('run.ps1 preserves PowerShell output capture and caller preferences', async () => {
  const context = setup({}, 3);
  const source = [unpack,
    '$PSNativeCommandArgumentPassing = "Legacy"; $PSNativeCommandUseErrorActionPreference = $true',
    '$parameters = $case.Parameters; $captured = @(& $case.Script @parameters); $code = $LASTEXITCODE',
    'if ($PSNativeCommandArgumentPassing -ne "Legacy" -or -not $PSNativeCommandUseErrorActionPreference) { throw "Launcher changed caller preferences" }',
    '[IO.File]::WriteAllText((Join-Path $case.Directory "captured.receipt"), ($captured -join "`n"))',
    'exit $code',
  ].join(';\n');
  const result = await invoke(context, source);
  assert.equal(result.status, 3, result.output);
  assert.match(readFileSync(path.join(context.dir, 'captured.receipt'), 'utf8'), /^\d+ \[INFO\] \[synthetic-native\] stdout receipt$/);
  assert(!result.stdout.includes('[synthetic-native] stdout receipt'), 'native output escaped the PowerShell capture');
});

scenario('run.ps1 preserves literal native arguments under Legacy passing', async () => {
  const context = setup({ CommandArguments: ordinaryArgs });
  const result = await direct(context, { legacyArguments: true });
  assert.equal(result.status, 0, result.output);
  assert.deepEqual(decode(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT), ordinaryArgs);
  checkLogs(context);
});

for (const [label, args, expected] of [
  ['option-looking native arguments', ['status', '--help', '--output', 'two words'], ['status', '--help', '--output', 'two words']],
  ['-Check', ['-Check'], ['doctor']],
  ['-Check extra', ['-Check', 'extra'], null],
]) {
  scenario(`run.ps1 handles ${label} from -File`, async () => {
    const context = setup();
    const result = await invoke(context, '', ['-NoLogo', '-NoProfile', '-File', context.data.Script, ...args]);
    assert.equal(result.status, expected ? 0 : 1, result.output);
    if (expected) {
      assert.deepEqual(decode(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT), expected);
    } else {
      assert.match(result.output, /-Check accepts no command arguments/);
      assert(!existsSync(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT));
    }
  });
}

scenario('run.ps1 reports a missing native binary', async () => {
  const result = await direct(setup({}, 0, { missingBinary: true }));
  assert.equal(result.status, 1, result.output);
  assert.match(result.output, /Native executable not found/);
});

for (const code of [0, 3, 7]) {
  scenario(`run.ps1 preserves native exit ${code} with native-error preference enabled`, async () => {
    const context = setup({}, code, { nativePreference: true });
    const result = await direct(context);
    assert.equal(result.status, code, result.output);
    assert(existsSync(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT));
  });
  scenario(`run.ps1 preserves native exit ${code} after one failed log write`, async () => {
    const context = setup({}, code, { nativePreference: true });
    const result = await direct(context, { failLog: true });
    assert.equal(result.status, code, result.output);
    assert(existsSync(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT));
    assert.equal(readFileSync(context.env.NUMERA_LAUNCHER_LOG_ATTEMPTS, 'utf8'), '1');
    assert.equal((result.stdout.match(timestampedWarning) || []).length, 1);
  });
}

scenario('run.ps1 continues after log directory creation fails', async () => {
  const context = setup({}, 7, { failedLogDirectory: true });
  const result = await direct(context);
  assert.equal(result.status, 7, result.output);
  assert(existsSync(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT));
  assert.equal((result.stdout.match(timestampedWarning) || []).length, 1);
});

for (const [label, code, options, expected] of [
  ['pauses after an Explorer failure', 7, { console: true, parent: 'explorer' }, ['Press Enter to close']],
  ['pauses after a failure with no parent', 3, { console: true, parent: null }, ['Press Enter to close']],
  ['does not pause on Explorer success', 0, { console: true, parent: 'explorer' }, []],
  ['does not pause for a shell parent', 7, { console: true, parent: 'pwsh' }, []],
  ['does not pause with real redirected stdio', 7, { parent: 'explorer' }, []],
  ...['Input', 'Output', 'Error'].map(stream => [`does not pause with only ${stream} redirected`, 7,
    { console: true, parent: 'explorer', redirected: stream }, []]),
]) {
  scenario(`run.ps1 ${label}`, async () => {
    const context = setup({}, code, options);
    const result = await direct(context);
    assert.equal(result.status, code, result.output);
    assert.deepEqual(decode(context.env.NUMERA_LAUNCHER_NATIVE_RECEIPT), ['menu']);
    assert.deepEqual(pauses(context), expected);
  });
}

after(() => {
  log(failures ? 'ERROR' : 'INFO', `Completed ${completed + failures} cases: ${completed} passed, ${failures} failed; no skips`);
  if (failures) {
    log('WARNING', `Retained failing synthetic launcher fixtures at ${root}`);
  } else {
    rmSync(root, { recursive: true, force: true });
    log('DEBUG', 'Removed owned launcher fixtures after successful checks');
  }
});
