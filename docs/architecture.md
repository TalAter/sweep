# Architecture

Sweep separates terminal interaction from execution. `tui` owns one Ratatui
session, from command entry through fetch, analysis, and approval. It returns a
decision and restores the terminal before `app` saves or executes a script.
Parsing, downloads, provider communication, and storage have independent modules;
the session coordinates them without owning persistence.

## Install command parsing

Shell words preserve quoted spaces, empty arguments, concatenated quoted segments,
and backslash escapes without evaluating a shell. The fetcher must name one HTTP(S)
URL word (also accepting curl's `--url=` form); URLs embedded in header text do not
select the installer. Multiple URL words and sudo options are refused rather than
silently changing the download or execution identity. Plain sudo remains supported.
Unescaped `$` (including double-quoted `$VAR`), unquoted leading `~`, and
arbitrary command substitution are refused; use explicit values or single-quoted
or escaped literals. The recognized `shell -c "$(fetcher ...)"` installer wrapper
remains supported without evaluating the substitution.
Runner shell options other than standalone `-s` are refused, rather than
converted to script arguments. Use `-s --` for script arguments starting with
`-` or `+`.

## Approval and analysis

Two isolated analysis passes run concurrently: one explains the script, the other
checks attempts to manipulate the reviewer. Only an explicitly clean manipulation
result allows an unqualified summary. Danger and untrusted analysis require typing
`install`; other resolved states default to Cancel.

Analysis failure stays visible in the review and does not prevent approval.
Detected manipulation always requires typing `install`, even when analysis fails.
Fetch failure ends the session because there are no script bytes to approve.
Both passes receive URL provenance and the command with recognized secret values
redacted. Analysis never executes downloaded content.
The Claude CLI disables built-in and MCP tools and loads an explicitly empty,
strict MCP configuration for both passes.

## Execution and persistence

Approved bytes are saved before they are sent to the shell. The shell receives
`-s --`, parsed arguments, and environment overrides; it can still prompt
through its controlling terminal. Startup and loader overrides are rejected,
shell lookup uses Sweep's PATH, and zsh user startup files are disabled.
Terminal UI and errors use stderr; `list` uses stdout. Installers are never
retried automatically.

SQLite records invocations separately from packages. A package is keyed by its
source URL and created only on the approved execution path. The package and a
`running` invocation commit together before spawning; the final invocation
result and package status update commit together. The installer owns a
foreground process group; signals reach its children, and Sweep restores
terminal ownership before recording the result. Signal handling stays active
through the final database transaction. An uncatchable termination leaves the
unfinished invocation as evidence of an attempt whose outcome is unknown. A
successful rerun preserves the first installation time; a failed rerun does not
erase a successful installation. Cancellation records the committed attempt
without creating a package or script file. Cancelling command entry before
submission records nothing.

Script files are keyed by SHA-256 and published atomically, so readers cannot see
partial writes. Invocation hashes are fingerprints, not foreign keys into that
cache. Download limits apply while streaming, including after decompression.

## Configuration

Configuration is read-only JSONC. `SWEEP_CONFIG` overlays whole top-level
fields, not nested provider entries. Provider keys can reference environment
variables. Absent providers produce a no-analysis state; misconfigured providers
display their failure reason. Neither implicitly approves execution. Canned
responses from `SWEEP_TEST_RESPONSES` are available only in debug builds.
Unreadable or malformed configuration stops startup with an error.
