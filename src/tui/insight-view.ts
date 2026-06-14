/**
 * Pure view-model for the install-approval dialog (step 3 of the LLM-insights
 * feature). It maps a step-2 `AnalysisResult` into the discriminated `InsightView`
 * the dialog component (step 4) renders — no Ink, no I/O, no side effects: this
 * is the dialog's brain, the component is its face.
 *
 * The dialog state is NOT the analysis severity. `Severity` is one input; the
 * state also turns on the manipulation pass (a non-clean pass outranks even a
 * `danger` severity) and on failure (→ analysis-failed) and on no provider (→
 * no-llm). So `InsightState` is its own type and the mapping is deliberate, not
 * identity.
 *
 * Two contracts are load-bearing enough to call out:
 *
 *   - Trust is an ALLOWLIST of `manipulation.kind === "clean"`. Anything else —
 *     `fired` OR `failed` (a thrown / aborted / timed-out pass) — renders the
 *     verdict under the compromise banner. Never denylist `fired`: a future
 *     non-clean kind would then silently render as trusted (a security regression).
 *     This mirrors the trust contract documented on `ManipulationPass`.
 *
 *   - Precedence: a failed *analysis* pass wins over the manipulation result. A
 *     "may be compromised" banner over a "couldn't analyze" body is incoherent —
 *     there is no verdict to caveat — so analysis-failed takes over even when the
 *     manipulation pass fired.
 *
 * `runAffordance` is named policy, not derivable chrome: danger and manipulation
 * require typing the confirm word; every other state is a plain button. It is
 * surfaced explicitly so a future change to which states need confirmation lives
 * here, not in the component.
 */

import type { AnalysisResult, Behavior } from "../installer/analyze.ts";

/** The literal word the danger/manipulation type-to-confirm input matches (trimmed, exact). */
export const CONFIRM_WORD = "install";

export type InsightState =
  | "clear"
  | "caution"
  | "danger"
  | "manipulation"
  | "no-llm"
  | "analysis-failed";

/** Plain `[Run]` button vs the type-`install`-to-run input (danger-level friction). */
export type RunAffordance = "button" | "type-confirm";

export type InsightView =
  | {
      state: "clear";
      source: string;
      verdict: string;
      flags: string[];
      behaviors: Behavior[];
      runAffordance: "button";
    }
  | {
      state: "caution";
      source: string;
      verdict: string;
      flags: string[];
      behaviors: Behavior[];
      runAffordance: "button";
    }
  | {
      state: "danger";
      source: string;
      verdict: string;
      flags: string[];
      behaviors: Behavior[];
      runAffordance: "type-confirm";
    }
  | {
      state: "manipulation";
      source: string;
      verdict: string;
      flags: string[];
      behaviors: Behavior[];
      banner: string;
      runAffordance: "type-confirm";
    }
  | { state: "no-llm"; source: string; message: string; runAffordance: "button" }
  | { state: "analysis-failed"; source: string; message: string; runAffordance: "button" };

/** Lowercase banner text; the component adds the leading `⚠` glyph, not us. */
const COMPROMISE_BANNER = "analysis may be compromised";

const NO_PROVIDER_MESSAGE = "No LLM provider configured — no analysis to show.";

/**
 * Display string for the install URL: `host + pathname`, no scheme, no query or
 * hash, with a lone trailing `/` trimmed. Deliberately not `slugFromUrl` — that
 * strips delivery prefixes and keeps only the first host label; the source line
 * wants the full host + path. If the URL can't be parsed, the raw string passes
 * through unchanged.
 */
function sourceDisplay(sourceUrl: string): string {
  let parsed: URL;
  try {
    parsed = new URL(sourceUrl);
  } catch {
    return sourceUrl;
  }
  const path = parsed.pathname === "/" ? "" : parsed.pathname;
  return `${parsed.host}${path}`;
}

export function deriveInsightView(result: AnalysisResult, sourceUrl: string): InsightView {
  const source = sourceDisplay(sourceUrl);

  if (result.kind === "noProvider") {
    return { state: "no-llm", source, message: NO_PROVIDER_MESSAGE, runAffordance: "button" };
  }

  const { analysis, manipulation } = result;

  // Precedence: a failed analysis pass has no verdict to caveat, so it outranks
  // the manipulation result entirely.
  if (analysis.kind === "failed") {
    return {
      state: "analysis-failed",
      source,
      message: `Couldn't analyze: ${analysis.reason}.`,
      runAffordance: "button",
    };
  }

  const { verdict, flags, behaviors } = analysis;

  // Trust ALLOWLIST: render the verdict normally only on a provably clean pass.
  // `fired` and `failed` both fall through to the compromise banner.
  if (manipulation.kind !== "clean") {
    return {
      state: "manipulation",
      source,
      verdict,
      flags,
      behaviors,
      banner: COMPROMISE_BANNER,
      runAffordance: "type-confirm",
    };
  }

  switch (analysis.severity) {
    case "clear":
      return { state: "clear", source, verdict, flags, behaviors, runAffordance: "button" };
    case "caution":
      return { state: "caution", source, verdict, flags, behaviors, runAffordance: "button" };
    case "danger":
      return { state: "danger", source, verdict, flags, behaviors, runAffordance: "type-confirm" };
  }
}
