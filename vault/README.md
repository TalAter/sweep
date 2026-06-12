---
name: README
description: Project brief — what sweep is, invariants, glossary, index of vault notes.
---

# Sweep

Sweep is a package manager for the long tail of dev tools that ship via `curl https://… | sh`. It gives those installs brew-grade ergonomics (`list`, `update`, `away`) without the publisher shipping a formula, and reads the script before you run it — surfacing what's about to happen and any red flags. Full spec: [[product-spec]].

This vault is the living reference: **what** Sweep does and **why** decisions were made — never **how** (that's in the code). Code wins on conflict; fix the note. `Source:` points at the code each note describes; `Last-synced` is the sha at which it was last reconciled.

This note is the single source for what sweep is. The root `README.md` restates it for GitHub rendering — when the pitch changes, update both; everything else links here.

**Before writing or restructuring any note, read [[vault-maintenance]].**

---

## Invariants

Always true. If a concept note contradicts one, the note is wrong.

1. **stdout is for payload only** (e.g. the `sweep list` table). Chrome goes to stderr; dialogs run in alt-screen.
2. **User-facing errors are plain language, prefixed `sweep:`.** Primitives throw; the orchestrator formats. Never-throws steps own their own lines: a step whose failures must not fail the install (script analysis, `installer/analyze.ts`) formats and prints its own `sweep:` line and swallows the error instead of throwing.
3. **Exactly one invocation row per `sweep "<cmd>"` run**, whatever the outcome.
4. **Sweep executes only the locally saved script, only after explicit approval.** Anything fetched for analysis (recursive scripts, binaries) is never run.
5. **TDD.** Failing test first.

---

## Glossary

Canonical vocabulary. Use consistently; do not invent synonyms. The distinctions are load-bearing.

- **Sweep home** — `~/.sweep/` or `$SWEEP_HOME`. The on-disk root for everything sweep persists.
- **DB** — `sweep.db` under sweep home. SQLite, two tables. Migrations gated by `schema_meta.version`.
- **Script store** — `cache/scripts/<sha256>` under sweep home. Content-addressed bytes — identical scripts dedupe to the same file.
- **Install command** — the parsed form of the user's `curl | sh` line: `envVars`, `sudo`, `shell`, `scriptArgs`, `url`. TS type: `InstallCommand`. Distinct from a "subcommand" — install commands are user input; subcommands are sweep verbs like `list`.
- **Invocation** — each time the user runs `sweep "<cmd>"`. Exactly one row in `invocations` regardless of outcome — parse failures and fetch failures count. `sweep list` and other non-install runs do *not* produce invocation rows.
- **Package** — one row in `packages`. Materializes the moment a fetch succeeds. Carries lifecycle status (see [[lifecycle]]).
- **Install** — an invocation that ended with `exitCode === 0`. Promotes the package's `status` to `installed`.
- **Slug** — human-readable identifier for a package. v0: derived from the URL host stem (`slugFromUrl`). v1: registry-canonical. Not deduplicated across packages — the `source_url` UNIQUE constraint disambiguates rows, and `sweep list` shows source URL alongside slug.
- **Runner shell** — the shell that executes the script (`sh`, `bash`, `zsh`). Right side of the pipe in the user's input.
- **Fetcher** — the tool the user typed to download the script (`curl`, `wget`). Sweep ignores it at exec time — sweep re-fetches itself, so the original fetcher's flags don't carry over.

---

## Index

- [[product-spec]] — what we're building and why; the CLI surface, analysis funnel, registry
- [[architecture]] — module shape, data flow, v1 seams
- [[lifecycle]] — package status machine, one-invocation-row-per-call invariant
- [[script-inspection]] — candidate install paths from LLM analysis, lstat resolution; existence over content
- [[conventions]] — cross-cutting code-style decisions
- [[vault-maintenance]] — rules for writing and rewriting vault notes
- [[wrap-core-api]] — shared substrate API; a directory symlinked from wrap-core — start at its README

`impl-specs/` holds per-feature implementation specs; each is deleted at its feature's compaction step.
