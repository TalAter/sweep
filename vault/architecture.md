---
name: architecture
description: Sweep's module shape, data flow, and where v1 plugs in. Code is the source of truth for internals; this is the bird's-eye view.
Source: src/
Last-synced: 701cf9d
---

# Architecture

Three layers wrap a SQLite + content-addressed file store.

- **Entry.** `index.ts` is two lines that call `main.ts`. `main.ts` looks up the first positional in the subcommand registry (currently only `list`); on miss it treats the positional as an install command and hands it to `runInstall`.
- **Subcommands.** `subcommands/{types,registry,dispatch,list}.ts`. Verb dispatch table. `install` is not a registered verb — the `sweep "<curl … | sh>"` shape has no verb keyword, so it's the implicit fallback.
- **Installer.** `installer/install.ts` is the orchestrator. It parses up front (direct mode), then drives the **install session** (`tui/install-session.ts`) — one alt-screen running fetch + two-pass analysis + the approval dialog — and resolves a decision (`run` / `cancel` / `fetch-failed`). It maps that decision to the CAS save + exec + the invocation row. Exec happens only on `run` (after approval); the alt-screen is torn down before exec. The primitives (`parse`, `fetch`, `exec`) don't know about each other.
- **Primitives.** `installer/{parse,fetch,exec}.ts`, `store/{db,packages,invocations,scripts}.ts`, `identity/naming.ts`, `fs.ts`, `config.ts`.

See the glossary in [[README]] for what packages, invocations, and installs *mean*; [[lifecycle]] for state transitions; [[conventions]] for code-style rules that span layers.

## Data flow

`sweep "<cmd>"`: argv → `main` → `runInstall` → (direct-mode parse) → install session (fetch + analysis + approval dialog under one alt-screen) → decision. On `run`: CAS save → `findOrCreatePackage` → exec → transactional commit of one invocation row + package status update.

`sweep list`: argv → `main` → `dispatch("list", …)` → `listCmd.run` → `listInstalledPackages` → stdout.

**Failure-channel asymmetry (deliberate).** A *fetch* failure tears down the alt-screen, prints a `sweep:` chrome line, and exits 1 — there's no script yet, so nothing to gate a dialog on. An *analysis* failure stays *in* the dialog as the `analysis-failed` state: there is a script to gate on, and analysis failing is sweep's failure, not the script's, so it must not block the run. Don't route fetch errors into the dialog to "unify" the two — the asymmetry is the design.

## Persisted shape

Sweep home is `~/.sweep/` (override `$SWEEP_HOME`):

- `sweep.db` — SQLite. Two tables: `packages` (one row per tracked tool, lifecycle status) and `invocations` (one row per `sweep "<cmd>"` run, including parse failures, fetch failures, and cancels).
- `cache/scripts/<sha256>` — raw script bytes, content-addressed. Identical scripts dedupe. `wx` (O_EXCL) writes give concurrent installers atomic dedup with no TOCTOU window.
- `config.jsonc` — user-authored JSONC, the provider-bearing config read at startup (`ensureConfig()`, overlaid by the `SWEEP_CONFIG` JSON env var). Sweep reads it but does not yet write it (no wizard).

No JSONL log.

## v1 seams

The places the architecture stretches without restructuring:

- **New verbs.** Add a file under `subcommands/` and register it. Install stays the implicit fallback.
- **Analysis + dialog.** Built. The install session (`tui/install-session.ts`) mounts one alt-screen and rerenders it in place: paste→loading→resolved for interactive mode, loading→resolved for direct mode (parse already happened). Beneath the loading spinner it runs fetch + two-pass analysis (`installer/analyze.ts`) — an analysis pass and a manipulation pass; the summary is trusted only when the manipulation pass came back provably clean — then swaps to the resolved `InsightDialog`, the approval gate. Both passes receive the same input: script bytes, fetch provenance (the typed URL, plus the post-redirect origin when it differs), and the redacted wrapping command — the literal `curl … | sh` line with secret env-var/arg values blanked in place (see [[product-spec]] for why the command, and not the machine, is fed). The analysis provider is resolved per run (`installer/analyze.ts` `resolveAnalysisProvider`): the `SWEEP_TEST_RESPONSES` test seam wins; otherwise the configured default provider drives a real LLM via `wrap-core/config` (`resolveProvider` + `llmFromResolved`). The no-provider dialog state is reached when config names no usable provider. What's still a *seam*: the **multi-source frame** — the LLM is the first and only wired insight source; deterministic signals (domain age, hash DBs, registry, diff-vs-prior) plug into the same dialog frame later — and the **first-run wizard** that auto-creates config when absent (mirroring wrap).
- **Config / first-run wizard.** `ensureConfig()` loads `config.jsonc` from disk via `wrap-core/config` (see [[wrap-core-api]]). The remaining seam is the first-run wizard that writes config when absent (mirroring wrap); the call site is unchanged when it lands.
- **Registry / canonical naming.** `slugFromUrl` has a stable signature; v1 swaps the body for a registry call.
- **Schema migrations.** `db.ts` reads `schema_meta(version)` and dispatches additive migrations — bump the version and append a migration block.
