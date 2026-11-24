# Launcher argument preservation recheck

## Scope and governing requirements

This follow-up starts from `c0b8368e9f34a8c02ce678d3b9b649172de10839` after
[PR #99](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/99) was merged.
Follow the owner's Rules, AGENTS instructions, hooks and prescribed Spec Kit
chain. The tracked contributor requirements remain in
[`CONTRIBUTING.md`](../../.github/CONTRIBUTING.md).

The owner deliberately replaced the two relaunching scripts with one
`run.ps1` that retains the original console. Do not restore `run-panel.ps1`,
Windows Terminal relaunch, or the removed relaunch switches to repair this
finding. The native executable and packaged application are unchanged.

This record connects assessment, requirements, implementation planning,
regressions and verification. It does not claim that an absent owner-specific
Spec Kit installation or external Rules file was executed or read.

## R8: PowerShell binding consumed native argument values

Severity: P2, incorrect command dispatch and loss of caller data.

The original script declared a parameter with
`[Parameter(ValueFromRemainingArguments = $true)]`. This makes the script an
advanced command and adds PowerShell common parameters. A token can therefore
be consumed before the script builds the native argument array. This happens
even when the token is a legitimate value for a native option.

Actual child-process reproductions against the original script included:

```powershell
.\run.ps1 build-collection --title -Debug
.\run.ps1 status -Verbose
.\run.ps1 build-collection --title -Check --data-dir -CommandArguments
```

The first invocation delivered only `build-collection` and `--title` to the
synthetic native executable; `-Debug` disappeared. The second dropped
`-Verbose`. Native option values matching the launcher's own declared names
also collided with binding. Quoting a string is not a general fix for this
wrapper boundary. `publish_cli.rs` accepts the string following `--title`
verbatim, so a nonempty title such as `-Debug` is valid native input.

The reproductions execute the actual launcher with an owned synthetic native
binary. They record argument count and length-preserving UTF-8 bytes, working
directory and exit status. They do not publish a Telegram pack or read an
operator's configuration.

## Required behavior

- With no native arguments, invoke `menu` exactly once.
- A leading `-Check` selects `doctor` and rejects additional native arguments.
  Preserve explicit Boolean and `SwitchParameter` forms and typed PowerShell
  splatting.
- Preserve the leading `-CommandArguments` convenience for an explicitly
  supplied string array, including an empty array or an empty string item.
- Once ordinary native command arguments begin, forward their complete tail
  in order. Common-parameter names, aliases and the launcher's own names are
  data there, not launcher controls.
- Preserve literal quotes, spaces, Unicode, timestamps and empty arguments;
  keep both `pwsh -File` and direct PowerShell caller invocation functional.
- Preserve the previous string-array binding distinction: a scalar explicit
  null or a bare empty array selects the default, while a null array item is
  an empty-string argument. Preserve positional array coercion before choosing
  menu versus a native command.
- Reject malformed or duplicate launcher controls before starting the native
  executable. Do not print the rejected value or complete argument list.
- Preserve native exit codes, pipeline capture, caller preference isolation,
  nonfatal log failure, and the existing interactive Explorer pause contract.

## Implementation plan and repair

Remove script-level parameter declarations so PowerShell cannot consume the
native namespace before the launcher sees it. Capture the automatic `$args`
array and resolve only the owned launcher-control prefix. Keep explicit array
payloads distinct from control tokens; preserve typed Boolean values from
hashtable splats. After the first ordinary native token, retain all remaining
arguments literally. Do not recover arguments by evaluating command text,
re-parsing `$MyInvocation.Line`, serializing dates through JSON, or building a
shell command string.

Normalize explicit and positional values using the previous string-array
contract before selecting the default command. Pass the resulting array to
the existing direct native invocation with
`PSNativeCommandArgumentPassing = 'Standard'`. Keep the script-local native
error preference and `$LASTEXITCODE` handling. Argument-validation failures use
the existing timestamped logger with INFO, WARNING, ERROR and DEBUG levels;
arguments and credentials remain absent from its messages.

The exact reviewed `run.ps1` SHA-256 in
`tools/test-inventory/language-policy.json` must follow the implementation.
This is a receipt update for that one permitted launcher, not a new language
exception or a relaxed policy rule.

## Regression and acceptance requirements

Retain the existing process assertions and add real process cases for:

1. Common parameter names and aliases as native arguments and option values,
   including `-Debug`, `-Verbose` and action/variable parameters.
2. Literal native `-Check` and `-CommandArguments` values after the command.
3. Both `-File` and PowerShell caller syntax, typed array and ordered hashtable
   splats, empty arrays/items, Boolean forms and explicit-plus-trailing input.
4. Duplicate/missing controls and `-Check` with extra native arguments, with
   proof that the native fixture never ran on rejection.
5. The retained menu/doctor/panel, exact argv/cwd, native exit, log-failure,
   pipeline-capture and pause assertions.

Run the complete suite, not a filtered replacement:

```powershell
node --test assets/tests/launcher-process.js
```

The existing main CI executes this command on both Windows architectures.
Keep that wiring mandatory and preserve the complete four-host native,
browser, Worker, package and security gates on the final published SHA.
Verify the declared PowerShell 7.3 behavior as well as the current runtime.
Real headless child processes are evidence for binding/argv behavior; supplied
Explorer and console observations do not establish graphical desktop coverage.

Use the PR's latest-commit checks for final acceptance. An earlier main run,
an inventory match, a queued job or a focused local pass cannot replace that
acceptance. If a failure occurs, diagnose its specific cause and require its
original postcondition again; do not blindly rerun or weaken the fixture.

## Primary references

- [PowerShell common parameters](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_commonparameters?view=powershell-7.6)
- [Advanced parameter attributes](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_functions_advanced_parameters?view=powershell-7.6)
- [Automatic `$args` variable](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_automatic_variables?view=powershell-7.6#args)
- [PowerShell argument parsing](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_parsing?view=powershell-7.6)
- [PowerShell splatting](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_splatting?view=powershell-7.6)
- [Spec Kit processes](https://github.com/github/spec-kit)
