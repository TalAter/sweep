---
name: impl-specs
description: How to write an implementation spec — the what/why/high-level-how record an implementer works from. Read before writing one.
---

# Writing an implementation spec

A spec in this directory (`impl-specs/<feature>.md`) is a per-feature, temporary artifact: written when a feature is planned, consumed by whoever implements it, deleted at that feature's compaction step (see [[vault-maintenance]]). It is **not** a concept note — concept notes describe current state and persist; a spec describes a change and is thrown away.

Its job: carry the **context an implementer needs that the code can't** — written for a reader with zero access to the planning conversation. If a decision, constraint, rejected alternative, or rationale isn't in the spec, it's lost. No "as we discussed".

## Put in

- **What to build, and why.** Each non-obvious decision with the framing that drove it — the tradeoff, the false-positive case, the philosophy, the alternative weighed and rejected. A bare decision with no why is one the implementer will "fix" by reverting it.
- **The how, at the altitude it was decided.** If the planning settled a technical approach, the interface or signature of a thing to build, the shape of a data flow, or how the pieces fit — record it. High-level how is design, and it belongs. The line is exact code and file lists (below), not "how" in general.
- **Out of scope.** What this feature deliberately doesn't do; what's deferred.
- **Design artifacts the conversation produced** — mockups, chosen signatures, examples. They earn their place.
- **Transitions.** before→after is expected ("supersedes X", "reverses invariant N", "what this reshapes"). The concept-note rules against change-log prose don't apply here — the spec's whole job is the change. At compaction the transition prose dies with the spec; the concept notes it feeds stay current-state.

## Leave out

- **Exact code.** Signatures and structure sketches yes; line-by-line implementations no.
- **Which files to touch.** The implementer maps the design onto the real tree — naming files pre-empts their judgment and goes stale fast. Point at an existing seam when it clarifies intent; don't dictate an edit list.
- **Speculation the planning didn't produce.** Don't invent test plans, error taxonomies, or schemas no one discussed. The implementer decides those against the real code — and if they find the feature must be built differently than the spec imagined, that's their call.

The line: **what + why always; how at the altitude it was actually decided — above exact-code, below file-by-file.**
