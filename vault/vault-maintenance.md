---
name: vault-maintenance
description: Rules for writing, rewriting, splitting, and compacting vault notes. Read before touching any vault file.
Source: vault/
Last-synced: d7ac86a
---

# Vault maintenance

Read before writing or restructuring any vault note.

## Concept note shape

```
---
name: <concept>
description: <one concrete line>
Source: <code paths>
Last-synced: <short sha>
---

# <Concept>

One paragraph: what + why.

## Sections as needed
- Link siblings: [[other]]
- Decisions inline under ## Decisions
- Types, signatures when load-bearing
- No code blocks
```

## Rules

- **What + why, never how.** How is in the code.
- **Terse.** Short sentences. Cut fluff. Sacrifice grammar over meaning. No repetition. Save tokens.
- **Current state only.** No change-log prose. Never "replaces X", "was moved from Y", "now uses Z instead of W". A note describes what exists now. Reasons are fine; the history of reaching them is not.
- **No pointer-to-moved-content lines.** If you split a note, don't leave "see [[X]] for the bit that used to be here" breadcrumbs.
- **Don't restate.** Don't describe what's already in [[README]], what's obvious from the module map, or what the reader sees by opening the source file.
- **Body does not repeat frontmatter.** `description` already said what the note is; don't open with the same sentence rephrased.
- **No code blocks.** Type signatures, structure sketches, literal error strings OK.
- **Atomic.** >~400 lines or two concepts → split.
- **Link, don't duplicate.** Concept Y in `y.md` → `[[y]]`, don't re-explain.
- **Present tense, no hedges.** No "currently" or "for now". When reality changes, rewrite.
- **Decisions inline, bulleted.** One line per decision + short reason. No header + paragraph per decision; that's bloat. Extract to its own note only when the decision spans concepts.
- **`Source:` is file-level.** Directories and files, not line ranges.
- **Don't overclaim absolute behavior.** A statement like "field X is absent when Y did not happen" must actually be true in every path — env vars, flags, re-runs, and on-demand section mounts can all set X independently.

## Create vs. extend vs. split

- Existing concept → extend during compaction.
- New concept → new note, link from [[README]] index.
- Small feature → fold into larger concept, delete impl spec.
- Note >~400 lines or mixed concerns → split, update links.

## Per-feature compaction

Runs on the feature branch **before** merging to main — the final step of the feature.

1. Rebase from main; resolve conflicts; tests pass. Then touch vault.
2. Read `impl-specs/<feature>.md`.
3. Identify affected concept notes.
4. **Rewrite** them. No appending. Goal: coherent current-state.
5. Update `Source:` and `Last-synced`.
6. Delete the impl spec.
7. If rewrite is unsafe or too big, stop. Leave `REWRITE-NEEDED: <one sentence>` in the impl spec. Escalate. No hallucinated fixes.

Mid-feature: impl spec is yours to churn. Concept notes only change at the compaction step above.

## General compaction

Human-triggered, occasional. A broad audit across the whole vault to catch drift that accumulated across several features — terminology that shifted, overlaps between notes, stale `Last-synced` shas, index entries pointing at notes that no longer match their scope. Not part of any one feature's branch; run independently when the vault feels saggy.

## Anti-patterns

- Append-only growth → rot. Rewrite.
- "For future reference" → delete. Ideas go in `ideas/`.
- Duplicating code, types, comments → link to source.
- "TODO" in concept notes → `ideas/todo.md`.
- Timestamps in prose → `Last-synced` already has this.
- Rewriting on a whim → only when shipped feature or compaction requires.
