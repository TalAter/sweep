# Architecture

`main` dispatches `list` or an install command. `tui` owns the Ratatui terminal,
interactive paste, background fetching and analysis, and the approval decision.
`app` persists that decision and executes only after terminal restoration.
`parse`, `redact`, `fetch`, `exec`, `config`, `analyze`, and `store` own their
respective boundaries. Project-specific UI and prompts live under `src/`.
No runtime or build dependency points at a sibling repository.

## Install flow

Parse → fetch → two concurrent, isolated analysis passes → explicit approval.
On approval: save script bytes → create/find package → execute → atomically
record invocation and package status. On decline: record the invocation only.
Analysis can fail without blocking approval; its failure stays visible in the
review. Fetch failure exits because there is no script to approve.

Terminal chrome and errors use stderr; `list` writes its payload to stdout.
The terminal is restored before the child shell runs, preserving terminal prompts.
The runner uses `-s --`, inherits the environment with parsed overrides, and
returns its exit code without retrying the installer.

## Compatibility and invariants

- SQLite keeps the existing `schema_meta`, `packages`, and `invocations` schema.
  Successful execution preserves the first installation timestamp and refreshes
  the content hash and last-run timestamp. Failed reinstallation keeps the
  package installed. `tracked` and `uninstalled` are legacy reserved statuses.
- Every committed attempt records one invocation, including operational errors.
  Cancelling interactive input before committing a command records none.
- JSONC configuration and the shallow `SWEEP_CONFIG` overlay remain compatible.
  Provider keys support `$ENV_VAR` references. Configuration is never rewritten.
- Both analysis passes see script bytes, URL provenance, and the command with
  recognized secret values redacted. Analysis never executes downloaded content.
- HTTP fetching has a 30-second deadline and a 5 MiB cap. The Rust implementation
  enforces the cap while streaming instead of allocating the entire response.
- CAS files are published atomically after the complete write, so concurrent
  readers cannot observe partial scripts. Existing filenames and bytes remain
  compatible. Invocation hashes are fingerprints, not CAS foreign keys.

These last two changes, and recording operational failures that previously lost
an invocation, correct implementation defects without expanding product scope.
