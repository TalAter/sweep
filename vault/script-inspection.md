---
name: script-inspection
description: LLM analysis emits candidate install paths; lstat resolves where a tool actually landed. Decided mechanism, not built.
---

# Script inspection

The install funnel (fetch → save → analyze → dialog → approve → exec) is specced in [[product-spec]]. This note pins how sweep knows *where* an install landed — without observing the filesystem.

## Mechanism

The LLM analysis returns, alongside its prose (summary, warnings, update/uninstall knowledge, unknowns), one machine-readable field: **candidate artifact paths** — every branch of the script's destination logic, as literal `~/…` paths (never `$HOME` or placeholders).

Resolution = `lstat()` each candidate; whichever exists is the install. Stat is cheap, so it runs:

- right after install — sweep is still alive when the script exits
- on every `list` — honest status: still installed vs removed externally
- retroactively — packages installed before analysis existed get analyzed from their script-store bytes

Candidates are static facts of the script: the *resolved* destination is env-dependent (installers sniff PATH/HOME), but the branches are enumerable from source. That's why no fs tracing, diffing, or snapshots are needed; sandbox detonation stays a registry-side future idea ([[product-spec]]).

## Decisions

- Analysis cached on **(script sha + raw command)**, not sha alone — sudo/shell/args change the verdict, so the command belongs in the key. `hash(sha + raw)` can only *miss* (re-run, cheap), never *collide* (serve a wrong verdict, unacceptable). Re-install of an identical command skips the LLM call; a changed upstream script (new sha) or any command edit forces fresh analysis, and a sha change is itself a signal to surface. finalUrl stays in the analysis prompt but out of the key — a dangerous redirect changes the bytes → new sha anyway, so origin-keying would only churn the cache (eu1/eu2/jp) for no security gain. Cross-user command normalization is a registry-side concern, deferred. (Built with the registry, not now.)
- **Existence is the invariant, not content.** Most tools self-update in place — binaries mutate, the recorded path stays. Never flag binary-content drift.
- `lstat`, not `stat` — a dangling symlink is still an artifact, and signals its target was removed.
- `unknowns` is the honesty valve: handoff writes (first-run state dirs, rc edits made by the installed binary) are invisible to script reading. Say so; don't guess.
- `list` says "tracked installs", not "installed" — it reports an existence check, not a guarantee.

## Build order

1. LLM provider + config working (prerequisite)
2. `list` wording change
3. Analysis-before-approval in the install flow, display-only — uninstall/update/registry wait for usage pressure
