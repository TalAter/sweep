---
name: architecture
description: Sweep's module shape, data flow, and where v1 plugs in. Code is the source of truth for internals; this is the bird's-eye view.
Source: src/
Last-synced: d7ac86a
---

# Architecture

Three layers wrap a SQLite + content-addressed file store.

- **Entry.** `index.ts` is two lines that call `main.ts`. `main.ts` looks up the first positional in the subcommand registry (currently only `list`); on miss it treats the positional as an install command and hands it to `runInstall`.
- **Subcommands.** `subcommands/{types,registry,dispatch,list}.ts`. Verb dispatch table. `install` is not a registered verb — the `sweep "<curl … | sh>"` shape has no verb keyword, so it's the implicit fallback.
- **Installer.** `installer/install.ts` orchestrates: parse → fetch → save (CAS) → resolve package → exec → persist (one transaction). The primitives (`parse`, `fetch`, `exec`) don't know about each other.
- **Primitives.** `installer/{parse,fetch,exec}.ts`, `store/{db,packages,invocations,scripts}.ts`, `identity/naming.ts`, `fs.ts`, `config.ts`.

See the glossary in [[README]] for what packages, invocations, and installs *mean*; [[lifecycle]] for state transitions; [[conventions]] for code-style rules that span layers.

## Data flow

`sweep "<cmd>"`: argv → `main` → `runInstall` → parse → fetch → CAS save → `findOrCreatePackage` → exec → transactional commit of one invocation row + package status update.

`sweep list`: argv → `main` → `dispatch("list", …)` → `listCmd.run` → `listInstalledPackages` → stdout.

## Persisted shape

Sweep home is `~/.sweep/` (override `$SWEEP_HOME`):

- `sweep.db` — SQLite. Two tables: `packages` (one row per tracked tool, lifecycle status) and `invocations` (one row per `sweep "<cmd>"` run, including parse and fetch failures).
- `cache/scripts/<sha256>` — raw script bytes, content-addressed. Identical scripts dedupe. `wx` (O_EXCL) writes give concurrent installers atomic dedup with no TOCTOU window.

No JSONL log. No config file in v0.

## v1 seams

The places the architecture stretches without restructuring:

- **New verbs.** Add a file under `subcommands/` and register it. Install stays the implicit fallback.
- **Analysis + dialog.** Slot between fetch and exec inside `runInstall` — step 5 (`installer/analyze.ts`) runs env-gated LLM analysis behind `SWEEP_TEST_RESPONSES`, a strict no-op for real installs until a production provider is flipped on; dialog remains unbuilt.
- **Config / first-run wizard.** `ensureConfig()` is a stub returning `{}`; the call site won't change shape when v1 reads from disk.
- **Registry / canonical naming.** `slugFromUrl` has a stable signature; v1 swaps the body for a registry call.
- **Schema migrations.** `db.ts` reads `schema_meta(version)` and dispatches additive migrations — bump the version and append a migration block.
