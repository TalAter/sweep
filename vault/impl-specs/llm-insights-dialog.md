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

Seven standalone units. **Steps 1–4 are DONE and in the code** — their build
instructions are trimmed below to "What Step N gives you" summaries of the
interfaces a later step consumes; the code is the source of truth. **Steps 5–7
remain** and keep full detail.

| # | Unit | State | Lands in |
|---|---|---|---|
| 1 | Severity theme tokens | **DONE** | `wrap-core` core theme + sweep `src/tui/theme.ts` |
| 2 | Two-pass analysis core | **DONE** | sweep `src/installer/analyze.ts` |
| 3 | Insight view-model (pure) | **DONE** | sweep `src/tui/insight-view.ts` |
| 4 | Dialog component + spinner | **DONE** | sweep `src/tui/insight-dialog.tsx` |
| 5 | Session controller | TODO | session module; threads abort into fetch |
| 6 | Orchestrator + `main` reshape (the flip) | TODO | `installer/install.ts`, `main.ts` |
| 7 | Vault + spec compaction | TODO | concept notes; delete this spec |

Dependencies: 5 needs 2+3+4 · 6 needs 5 · 7 last.
Each step: TDD (failing test first), `/code-review` after, keep every commit working.

### ⚠ Repo & dependency setup (read before touching dependencies)

This sweep checkout is a **git worktree** at
`sweep/.claude/worktrees/llm-insights` whose `node_modules` is a symlink to the
**main checkout's** `node_modules` (`~/mysite/sweep/node_modules`). `wrap-core`
is a **local symlink** into the sibling checkout `~/mysite/wrap-core`, carrying
**uncommitted** work on branch **`core-severity-tokens`** (the Step-1 severity
tokens) — wrap's matching half is on `core-severity-tokens` in `~/mysite/wrap`
too. Don't "fix" those uncommitted-to-`main` repos by reverting.

Because `package.json` declares `wrap-core: 0.0.1` (a placeholder, not published),
**`bun add` from sweep fails** (`registry.npmjs.org/wrap-core 404`). Dependencies
are resolved only by the local-only Bun workspace **`~/mysite/monowrapo`** (its
`workspaces` lists `../wrap-core`, `../wrap`, `../sweep`). To add a dep: edit the
owning package's `package.json` by hand, then `bun install` **from
`~/mysite/monowrapo`** (it symlinks `wrap-core` locally and writes into the
shared `node_modules`). `monowrapo` references the **main** checkout (`../sweep`),
so the dep line must be present in the main checkout's `package.json` at install
time — `ink-testing-library` (Step 4) is already installed and present in both
checkouts' manifests. Steps 5–7 are not expected to need new deps.

### What Step 1 gives you (theme — built)

The danger/warning palette was a **cross-repo promotion** (byte-identical in wrap
and sweep; `CoreThemeTokens` had no red token). It now lives in **`wrap-core`**:
`CoreThemeTokens.severity = { warning: {frame, pill}, danger: {frame, pill} }`,
populated in `DARK_CORE`/`LIGHT_CORE`. wrap's `risk.medium/high` and sweep both
derive from it. (Branch caveat above.)

sweep `src/tui/theme.ts` exposes:
- `getSeverityPreset(level: "caution" | "danger") → { stops: Color[]; pill: PillSegment }`
  — `caution`→core `warning`, `danger`→core `danger`; pill labels `"⚠ caution"` /
  `"✗ danger"`, bold. **No `clear` preset** (clear has no pill — deliberate). The
  preset reads the **module-global** theme via `getTheme()` (set by `setTheme` in
  `mount.ts` at runtime; defaults to `DARK_CORE`, never throws).
- `DARK_GRADIENT` / `LIGHT_GRADIENT` — the neutral blue frame for the **no-pill**
  states (clear / no-LLM / analysis-failed / manipulation / loading / paste).
  Sweep-local (dark variant differs from wrap's, so not promoted).

### What Step 2 gives you (analysis core — built)

`src/installer/analyze.ts` exports `analyzeScript({ url, scriptBytes, signal? })
: Promise<AnalysisResult>` — runs the **analysis** and **manipulation** passes
concurrently as two isolated conversations, **never throws**, abortable via
`signal`. Exported types:

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

- **`noProvider` is top-level** (the no-LLM signal). **Trust contract:** only
  `manipulation.kind === "clean"` is provably clean. Allowlist `clean`; never
  denylist `fired`.
- Test seam: `SWEEP_TEST_RESPONSES` is a JSON object `{ analysis, manipulation }`
  (each feeds its own `createLlm({name:"test"})` handle). Absent → `noProvider`;
  present-but-malformed → `analyzed` with both passes `failed`.
- The **legacy** `maybeAnalyzeScript` (single-pass, `{summary}` schema, stderr) is
  still wired into `runInstall` step 5 and reads the *flat* `SWEEP_TEST_RESPONSES`
  shape. **Step 6 removes it** — leave it alone until then.
- `ensureConfig()` returns `{}` (no real provider this phase), so the no-LLM state
  is reachable only via the test seam.

### What Step 3 gives you (view-model — built)

`src/tui/insight-view.ts` — a pure module (no Ink, no I/O). It is the dialog's
**brain**; it owns *words + structure + affordance policy*. Step 4's component
owns *glyphs/color/layout*. Exports:

```ts
const CONFIRM_WORD = "install"; // the danger/manipulation type-to-confirm literal (trimmed-exact match)
type InsightState = "clear" | "caution" | "danger" | "manipulation" | "no-llm" | "analysis-failed";
type RunAffordance = "button" | "type-confirm";
type InsightView =        // discriminated on `state`
  | { state: "clear";           source; verdict; flags; behaviors; runAffordance: "button" }
  | { state: "caution";         source; verdict; flags; behaviors; runAffordance: "button" }
  | { state: "danger";          source; verdict; flags; behaviors; runAffordance: "type-confirm" }
  | { state: "manipulation";    source; verdict; flags; behaviors; banner; runAffordance: "type-confirm" }
  | { state: "no-llm";          source; message; runAffordance: "button" }
  | { state: "analysis-failed"; source; message; runAffordance: "button" };
function deriveInsightView(result: AnalysisResult, sourceUrl: string): InsightView;
```

Encoded contracts (all unit-tested):
- **Precedence:** a failed *analysis* pass (`analysis.kind==="failed"`) → `analysis-failed`,
  and it **wins over** the manipulation result (a "may be compromised" banner over a
  "couldn't analyze" body is incoherent).
- **Trust allowlist:** analysis-ok + manipulation not `clean` (fired OR failed) →
  `manipulation` state (`banner: "analysis may be compromised"`, type-confirm friction),
  regardless of the untrusted severity. Only `clean` renders the verdict trusted.
- **Affordance policy** is exposed explicitly (`runAffordance`): danger + manipulation
  → `type-confirm`; everything else → `button`. Kept as a named field (not re-derived in
  the component) so a future change to which states need friction lives here.
- **`source`** = `host + pathname` of the URL (no scheme/query/hash; lone trailing `/`
  trimmed; raw string fallback if `new URL` throws). Deliberately **not** `slugFromUrl`
  (that keeps only the first host label).
- The exact copy strings live here: no-llm `"No LLM provider configured — no analysis
  to show."`; analysis-failed `` `Couldn't analyze: ${reason}.` ``; banner text (the
  `⚠` glyph is added by the component).

> **Known edge (accepted, not reconciled):** the analysis schema lets the model return
> `severity:"clear"` with a **non-empty** `flags` array, so a `clear` view can carry
> flags. The component renders them (a neutral header + a Flags section). Current call:
> show them — don't hide model output. If this reads as incoherent in practice, the fix
> belongs in the view-model (force `flags:[]` on clear, or bump clear+flags → caution),
> not the renderer. No test pins clear-with-flags yet.

### What Step 4 gives you (dialog component — built)

`src/tui/insight-dialog.tsx` — presentational + interactive Ink component. Driven
entirely by props; **does not fetch/analyze** (Step 5 does). Exports:

```ts
type InsightDialogState =
  | { phase: "loading"; source: string }       // pre-/mid-analysis; spinner; only Cancel live
  | { phase: "resolved"; view: InsightView };   // the resolved dialog
type InsightDialogProps = {
  state: InsightDialogState;
  neutralGradient: Color[];   // appearance-resolved by the caller (DARK_/LIGHT_GRADIENT)
  onRun: () => void;
  onCancel: () => void;
};
function InsightDialog(props: InsightDialogProps): JSX.Element;
```

Behaviors (in code; pinned by `tests/tui-insight-dialog.test.tsx`):
- Frame: severity stops for caution/danger via `getSeverityPreset`; **neutral** for
  everything else **including manipulation** (severity untrusted → the banner, not the
  frame, carries the alarm; banner text uses `getSeverityPreset("danger").pill.fg`).
- Header pill is rendered **inline** as the first content line next to the source — a
  deliberate divergence from wrap, which puts its risk pill in the Dialog *border*.
- Body: verdict, then Flags (`⚠ <flag>`, only if non-empty), then `Appears to do (not
  exhaustive):` with `• <desc>` and inline `(sudo)`; no-llm/analysis-failed show
  `message`; loading shows `<spinner> Analyzing…` via Ink `useAnimation` +
  `SPINNER_FRAMES`/`SPINNER_INTERVAL` from `wrap-core/chrome`, `isActive`-gated to loading.
- Interaction: Esc / Ctrl+C → `onCancel`; button states `[Cancel] [Run]` with focus
  defaulting to Cancel (arrows move, Enter fires focused); danger/manipulation render a
  focused `TextInput` whose submit calls `onRun` only when `value.trim() === CONFIRM_WORD`.
  The type-confirm input is **stacked** (Cancel row, then prompt line, then input) —
  `TextInput`'s `InputFrame` is `width:100%`, so it can't sit inline after `[Cancel]`.
  (The spec's old inline danger sample was therefore wrong and has been dropped.)
- Substrate: built on `wrap-core/tui` `Dialog`/`Pill`/`ActionBar`/`TextInput`/
  `useKeyBindings`/`useTheme`. Tests use `ink-testing-library` + `stripAnsi` +
  `tests/helpers.ts` `waitFor` (poll-until-assert; the house idiom — don't use fixed
  sleeps).

**Contracts the Step-5 controller must honor** (the component can't enforce these; they
came out of Step-4 review):
- **Ctrl+C is dead unless the app opts in.** Ink's `exitOnCtrlC` kills the process before
  the component's Ctrl+C binding fires, so `onCancel` (→ abort + `cancelled` outcome)
  won't run. The controller's Ink app **must set `exitOnCtrlC: false`** (or register an
  exit teardown) so Ctrl+C routes through `onCancel`.
- **Reset local UI state across the loading→resolved transition.** The component holds
  `confirmValue`/`focusedIndex` in `useState` with no reset. The current one-way
  transition is safe (loading mounts no input and leaves focus at Cancel), but to keep
  "focus defaults to Cancel" robust, the controller should **remount on phase change via
  a React `key`** (or the component should grow a reset). wrap's `response-dialog.tsx`
  resets on its state tag — mirror that.
- **`onRun` can double-fire.** After `onRun`, the dialog stays live (a second Enter
  re-fires it) until unmounted. The controller must **unmount synchronously on `onRun`**
  (or introduce a "submitting" phase) before exec.
- The interactive-mode **inline parse error** ("return to the paste input with an error")
  belongs to the **paste** component (`interactive.tsx`), not `InsightDialog` — its prop
  type has no room for it, by design.

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
designed so the deterministic signals drop in later as additional sources without
reshaping the dialog. That is why the no-LLM state still shows a dialog rather
than reverting to silent exec — the gate is universal; only its *contents* depend
on which sources are configured.

## "Confirm, don't block" (the model behind Steps 3–4, still governs Step 7)

The realized affordances (built in 3+4): **clear** → no pill, `[Run]`; **caution** →
`⚠` pill, `[Run]`; **danger** → `✗` pill, type-`install` to run; **manipulation** →
`⚠ analysis may be compromised` banner + type-`install`; **no-LLM** / **analysis-failed**
→ source-only header, plain `[Run]` (analysis-failed is sweep's failure, not the
script's — no friction). Focus defaults to Cancel everywhere except the type-confirm
input (which must hold focus to be typeable).

**There is no hard Block state** — a deliberate departure from product-spec's Allow/Block
two-state design. An LLM false-positive that *removes* the ability to run is too hostile
for a probabilistic, single-source judgment whose stance is "visibility, not verdicts."
Friction (typing `install`) replaces removal; the deliberation cost is paid only on
danger/manipulation. Type-to-confirm beats double-press/focus-distance as the most
deliberate option (mirrors "type the repo name to delete"). **This supersedes
product-spec's "The install dialog" section (its Allow/Block split, the Read-source
button, the signal-panel `✓/◯` rows) — reconcile product-spec when this lands (Step 7).**

## Step 5 — the session controller (the reshape, in full)

`architecture.md` still frames analysis as a stderr-only "step 5" with the dialog
"unbuilt." This phase changes the install orchestrator's shape. Today (read the code):
`main` (`src/main.ts`) interactive path calls `promptInstallCommand()`
(`src/tui/mount.ts`), which opens **and tears down** its own alt-screen via
`openDialog`, returns the pasted string, and *then* `main` calls `runInstall`
(`src/installer/install.ts`) — a linear parse→fetch→save→analyze→exec function with no
TUI awareness. A single alt-screen session spanning paste → fetch → analyze → approve
cannot live inside that boundary.

So the controller (Step 5) owns a **single alt-screen session** that transitions through
states (this is why it needs a session-style controller, not `mount.ts`'s
open-await-one-close shape):

- **Interactive mode** (`sweep`, no arg): the paste dialog (`interactive.tsx`)
  **rerenders in place** into the loading spinner, then into the resolved `InsightDialog`
  — one continuous session; the paste dialog *becomes* the insights dialog. A pasted
  command that fails to **parse** surfaces **inline** in that session (back to the paste
  input with an error; let the user re-paste) — not as a `sweep:` stderr line that would
  be hidden behind the live alt-screen.
- **Direct mode** (`sweep "curl … | sh"`): parse already happened on the argument, so the
  dialog appears with the spinner straight away; a direct-mode parse failure keeps today's
  pre-dialog chrome + exit (no dialog mounted yet).

Under the session, the controller runs **`fetchScript` + `analyzeScript` concurrently/in
sequence beneath one spinner** ("Analyzing…" covers fetch + both passes), then swaps the
`InsightDialog` `state` from `{phase:"loading", source}` to `{phase:"resolved", view}`
where `view = deriveInsightView(analysisResult, url)`. During loading only Cancel is live.

**Abort wiring.** `fetchScript(url)` today has **no external signal** — it builds its own
`AbortController` only for the 30s timeout. Thread an external `AbortSignal` into it (so
the controller can abort an in-flight fetch), and pass the same signal into `analyzeScript`
(which already takes one). Cancelling at any point — **including during the spinner** —
aborts the in-flight fetch + LLM sends.

**Cancel must win over abort-induced failure.** `analyzeScript` never throws; an abort maps
to `analysis.failed` *inside* it. So a cancel that aborted the passes must **short-circuit
to the cancel outcome + teardown** — it must NOT flash a `analysis-failed` ("Couldn't
analyze: aborted") dialog. Resolve cancel before the controller would render the
abort-as-failed view.

**Honor the Step-4 contracts above:** `exitOnCtrlC:false` (so Ctrl+C → onCancel →
abort+cancel), remount-on-phase-change via `key` (focus reset), and unmount synchronously
on `onRun` (no double-fire) before handing control back for exec.

**Boundary with Step 6 (implementer's call, design coherently across both):** the natural
split is for the controller to own the *TUI session + the two async operations it drives*
and resolve to a decision the orchestrator acts on — e.g. `run` (with the fetched bytes /
sha) vs `cancel` (with whether fetch had completed) — while Step 6 owns DB rows, the CAS
save, exec, and outcome recording. The fetch result must flow out of the session to the
orchestrator. This is intent, not a prescription; it *is* a reshaping, not an insertion.

## Step 6 — orchestrator + the flip; outcomes (in full)

Rewire `main` + `runInstall` so fetch + analysis run **under** the live session, the gate
sits between fetch and exec **within** it, and the alt-screen is **torn down before
`exec`** (the child inherits the real terminal). Step 6 also **removes** the legacy
`maybeAnalyzeScript` and its tests (`tests/installer-install.test.ts` section 8, "step-5
analysis (env-gated)"), replacing that call with the session-driven `analyzeScript`.

**The invocation log (run outcomes).** The `invocations` table logs *every run* — ran,
errored, or not run. These are **run outcomes**, distinct from a package's lifecycle
**state**; this phase only writes the log. Current `runInstall` writes rows for
`parse_failed` (exit 2), `fetch_failed` (exit 1), `ran` (exit 0), `errored` (non-zero),
each with one row (invariant 3), `tsStarted` captured at entry. Add:

- **Cancel / decline → the `cancelled` outcome** (already in the `Outcome` union, in
  product-spec, and in conventions.md; no code path produces it yet; the free-text column
  means no migration). Preserves invariant 3 (one row per run). **Exit code:** non-zero,
  outside the *fixed* parse(2)/fetch(1) codes; exec passes arbitrary codes through, so
  collision with *those* can't be avoided and isn't worth chasing — **130** is the working
  default.
- **Cancel applies at any point the dialog is up, including the spinner.** If cancel lands
  **before fetch completed** there is no sha/package yet — record the row with the same
  null-field shape the fetch-failure path uses.
- **Run → exec as today**, outcome `ran` / `errored`.

**Package "state" after a cancel is a separate concern — deferred.** A package row
materializes at fetch success (status `attempting`); a cancelled run never reaches exec,
so where that row should rest is an open *lifecycle* question. Do **not** settle it: don't
add a cancel transition to [[lifecycle]], and don't change `sweep list`. Just log the
`cancelled` run.

## Step 7 — vault + spec compaction (in full)

- **Invariant 2 needs a real rewrite, not a wording nudge.** It was amended (commit
  `032ef44`) to mandate that the analysis step "formats and prints its own `sweep:` line
  and swallows the error instead of throwing." This phase **reverses that mechanism**:
  analysis failure now surfaces **in the dialog** (the analysis-failed state), not on
  stderr — a `sweep:` line would be hidden behind the alt-screen. Analysis still never
  throws and never fails the install; only the failure *channel* changes.
- **Reconcile [[product-spec]]'s "The install dialog" section** with "confirm, don't
  block" (above): its Allow/Block split, Read-source button, and signal-panel `✓/◯` rows
  are superseded or deferred.
- **Reframe `architecture.md`'s "step 5"** (analysis is no longer stderr-only; it drives
  the gate).
- **[[lifecycle]]:** note that a cancel logs a *run* only (no state transition this phase).
- **Stale concept-note props:** `wrap-core/vault/wrap-core-api/tui.md` lists `Pill` as
  taking `segs` and `ActionBar` `focused`; the real props are `PillProps` (`label/fg/bg/
  bold/icon/nerdFonts`) and `focusedIndex`. Fix when convenient.
- **Delete this spec.**

## Out of scope (this phase)

Deliberately deferred — do **not** build these now:

- **Provider config + first-run wizard.** Assume `~/.sweep/config.jsonc` already defines a
  provider (tracked in [[script-inspection]]; mirror wrap's config-wizard when built). The
  dialog is exercised via the test provider (`SWEEP_TEST_RESPONSES`); `ensureConfig()`
  returns `{}` today so the **no-LLM state** is only reachable via the test seam.
- **`--auto` and non-TTY policy.** Assume an interactive TTY.
- **Read source / source viewer.** No button this phase.
- **Analysis caching/persistence.** Recompute both passes every run; no new DB storage.
- **All non-LLM insight sources** — domain age, hash/sigstore lookups, registry, diff-vs-prior.
- **Install-location resolution** ([[script-inspection]]'s candidate-paths + `lstat`). The
  analysis schema deliberately does not carry candidate paths yet.
- **Package lifecycle "state" after a cancel** (and any `sweep list` display of it).

## Sample dialogs (design artifacts — clear / no-LLM / loading)

The pill/source header and the body sections are normative; the danger type-confirm sample
was dropped because the real input stacks (see "What Step 4 gives you").

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
│                                                       │
│  [ Cancel ]   [ Run ]                                 │
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
