---
name: lifecycle
description: Package status state machine + the "exactly one invocation row per call" invariant. The two semantic decisions hardest to read off the code.
Source: src/store/packages.ts, src/installer/install.ts, src/store/invocations.ts
Last-synced: d7ac86a
---

# Lifecycle

Two semantic invariants the code enforces but doesn't shout about. See the glossary in [[README]] for what these states mean.

## Package status

A `packages` row's `status` moves through:

- **`attempting`** — set by `findOrCreatePackage` on first sight of a `source_url`. The row exists; nothing has run yet.
- **`installed`** — exec returned 0. Refreshes `current_sha256`, sets `installed_at` *iff still NULL* (preserves the first-install timestamp across re-runs), bumps `last_ran_at`.
- **`failed`** — exec returned non-zero AND prior status was `attempting`. `installed` packages never downgrade. `current_sha256` and `installed_at` stay put; only `last_ran_at` advances.

The decision: re-running a failing script against an `installed` package keeps it installed and records the failure as an invocation row only. The user knows it worked once; a transient failure shouldn't erase that.

`uninstalled` and `tracked` are reserved for v1 (`sweep away`, `sweep track`) — never set in v0.

The whole transition is one UPDATE with CASE expressions, so there's no read-modify-write window between concurrent installers.

## Exactly one invocation row per call

Every `sweep "<cmd>"` writes one — and only one — row to `invocations`. The `outcome` column discriminates:

- **`parse_failed`** — `parseInstallCommand` rejected the input. No package, no url, no sha.
- **`fetch_failed`** — `fetchScript` threw. Url + parsed install-command JSON recorded; no package, no sha.
- **`ran`** — exec returned 0.
- **`errored`** — exec returned non-zero.

`tsStarted` is captured at the top of `runInstall` (before parse) and threaded through every code path — even `parse_failed` rows reflect when the user actually pressed enter, not when sweep got around to writing the row.

The happy-path `invocations` INSERT and `packages` UPDATE share one transaction, so a partial write is unobservable.
