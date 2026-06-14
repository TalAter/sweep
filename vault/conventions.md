---
name: conventions
description: Cross-cutting code-style decisions in sweep — camelCase boundary, error prefix, exit codes, test seams, mirror-wrap default.
Source: src/, tests/
Last-synced: 110435b
---

# Conventions

House rules that span layers. Deviating wants a reason.

- **camelCase TS, snake_case SQL.** Domain types (`PackageRow`, `Invocation`) expose `sourceUrl`, `currentSha256`, `tsStarted`. SQL columns stay `source_url`, `current_sha256`, etc. The store layer mediates at the boundary (`rowToPackage` and similar). SQL column names never leak above `src/store/`.
- **`Outcome` codes stay snake_case.** `ran | cancelled | errored | parse_failed | fetch_failed` are persisted strings — renaming them would be a migration. The TS `Outcome` union mirrors the on-disk values verbatim.
- **Error prefix.** User-facing stderr lines start with `sweep:`. The prefix lives in the orchestrator (`installer/install.ts`) and `main.ts`'s top-level catch — primitives throw or return typed errors, the orchestrator formats them.
- **Exit codes.** `2` parse failure; `1` fetch failure or unexpected throw; `130` cancel/decline of a committed install; `0` when an interactive paste is cancelled before any command is committed; otherwise pass through the script's exit code. `runInstall` returns the code; `main.ts` assigns it to `process.exitCode` rather than calling `process.exit()` so in-process tests can `await main()`.
- **Test seams use `__name` exports.** `__resetForTests` (closes the cached DB handle), `__setTimeoutMs` (fetch timer override). Borrowed from wrap. Importing them outside tests is a smell.
- **Test isolation via preloads.** `tests/sweep-home-preload.ts` pins `SWEEP_HOME` to a fresh tmpdir per test and calls `__resetForTests` so the cached DB handle doesn't point at a deleted file. `tests/spawn-inherit-preload.ts` rewrites `stdout/stderr: "inherit"` → `"ignore"` so spawned scripts don't leak into the test reporter. Both registered in `bunfig.toml`.
- **Mirror wrap when sweep has no opinion.** Sweep is wrap's sibling, sharing substrate via wrap-core (see [[wrap-core-api]]). Conventions start there and diverge only when domain pressure forces it — e.g., sweep's `Subcommand` shape is deliberately simpler than wrap's `CLIFlag` union because v0 has one verb.
- **Default to no comments.** Comments only where the WHY is non-obvious (a hidden constraint, a subtle invariant, a workaround). Identifiers carry the WHAT. PR descriptions carry the THIS-TASK context.
