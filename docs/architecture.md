# Architecture

Sweep separates terminal interaction from execution. `tui` owns one Ratatui
session, from command entry through fetch, analysis, and approval. It returns a
decision and restores the terminal before `app` saves or executes a script.
Parsing, downloads, provider communication, and storage have independent modules;
the session coordinates them without owning persistence.

## Approval and analysis

Two isolated analysis passes run concurrently: one explains the script, the other
checks attempts to manipulate the reviewer. Only an explicitly clean manipulation
result allows an unqualified summary. Danger and untrusted analysis require typing
`install`; other resolved states default to Cancel.

Analysis failure stays visible in the review and does not prevent approval.
Fetch failure ends the session because there are no script bytes to approve.
Both passes receive URL provenance and the command with recognized secret values
redacted. Analysis never executes downloaded content.

## Execution and persistence

Approved bytes are saved before they are sent to the shell. The shell receives
`-s --`, parsed arguments, and environment overrides; it can still prompt through
its controlling terminal. Terminal UI and errors use stderr; `list` uses stdout.
Installers are never retried automatically.

SQLite records invocations separately from packages. A package is keyed by its
source URL and created only on the approved execution path. The invocation result
and package status update commit together. A successful rerun preserves the first
installation time; a failed rerun does not erase a successful installation.
Cancellation records the committed attempt without creating a package or script
file. Cancelling command entry before submission records nothing.

Script files are keyed by SHA-256 and published atomically, so readers cannot see
partial writes. Invocation hashes are fingerprints, not foreign keys into that
cache. Download limits apply while streaming, including after decompression.

## Configuration

Configuration is read-only JSONC. `SWEEP_CONFIG` overlays whole top-level fields,
not nested provider entries. Provider keys can reference environment variables.
Missing or invalid provider entries produce an explicit no-analysis state; they
never implicitly approve execution. Unreadable or malformed configuration stops
startup with an error.
