/**
 * `deriveInsightView` — the pure view-model (step 3 of the LLM-insights dialog
 * feature). It maps a step-2 `AnalysisResult` into the discriminated `InsightView`
 * the install-approval dialog (step 4) renders. No Ink, no I/O — data only.
 *
 * The contracts under test are the state matrix and its precedence rules:
 *   - noProvider          → no-llm
 *   - analysis failed     → analysis-failed (WINS over any manipulation result)
 *   - analysis ok + manipulation.kind === "clean" (TRUST ALLOWLIST, never a
 *     denylist of "fired") → severity-driven clear / caution / danger
 *   - analysis ok + manipulation NOT clean (fired OR failed) → manipulation banner
 * plus the `source` display string derived from the install URL, and the
 * `runAffordance` policy (danger + manipulation type-to-confirm; else button).
 *
 * Fixtures are harmless if executed (echo/true-style descriptions, neutral
 * `sudo` booleans) per testing.md — this module never execs, but the rule holds.
 */

import { describe, expect, test } from "bun:test";
import type { AnalysisResult, Behavior, ManipulationPass } from "../src/installer/analyze.ts";
import { deriveInsightView } from "../src/tui/insight-view.ts";

const URL = "https://ollama.com/install.sh";

const BEHAVIORS: Behavior[] = [
  { description: "Download a binary from ollama.com", sudo: false },
  { description: "Install to /usr/local/bin/ollama", sudo: true },
];

/** An `analyzed` result with an ok analysis pass at the given severity. */
function ok(
  severity: "clear" | "caution" | "danger",
  manipulation: ManipulationPass,
): AnalysisResult {
  return {
    kind: "analyzed",
    analysis: {
      kind: "ok",
      severity,
      verdict: "Ollama LLM runtime. Standard vendor installer from the official domain.",
      flags: severity === "clear" ? [] : ["Requests sudo"],
      behaviors: BEHAVIORS,
    },
    manipulation,
  };
}

describe("deriveInsightView — state matrix", () => {
  test("noProvider maps to no-llm with the exact no-provider message and a button", () => {
    const view = deriveInsightView({ kind: "noProvider" }, URL);

    expect(view.state).toBe("no-llm");
    if (view.state !== "no-llm") return;
    expect(view.message).toBe("No LLM provider configured — no analysis to show.");
    expect(view.runAffordance).toBe("button");
  });

  test("ok clear + clean manipulation: clear state, button, verdict + behaviors pass through", () => {
    const view = deriveInsightView(ok("clear", { kind: "clean" }), URL);

    expect(view.state).toBe("clear");
    if (view.state !== "clear") return;
    expect(view.runAffordance).toBe("button");
    expect(view.verdict).toBe(
      "Ollama LLM runtime. Standard vendor installer from the official domain.",
    );
    expect(view.behaviors).toEqual(BEHAVIORS);
    expect(view.flags).toEqual([]);
  });

  test("ok caution + clean manipulation: caution state, button, flags + behaviors pass through", () => {
    const view = deriveInsightView(ok("caution", { kind: "clean" }), URL);

    expect(view.state).toBe("caution");
    if (view.state !== "caution") return;
    expect(view.runAffordance).toBe("button");
    expect(view.flags).toEqual(["Requests sudo"]);
    expect(view.behaviors).toEqual(BEHAVIORS);
  });

  test("ok danger + clean manipulation: danger state with type-to-confirm affordance", () => {
    const view = deriveInsightView(ok("danger", { kind: "clean" }), URL);

    expect(view.state).toBe("danger");
    if (view.state !== "danger") return;
    expect(view.runAffordance).toBe("type-confirm");
  });

  test("ok clear + manipulation FIRED: manipulation outranks the clear severity (allowlist, not severity, decides)", () => {
    // Severity is clear, yet the manipulation pass is not clean, so the verdict
    // is suspect: the manipulation banner + danger-level friction win.
    const view = deriveInsightView(ok("clear", { kind: "fired" }), URL);

    expect(view.state).toBe("manipulation");
    if (view.state !== "manipulation") return;
    expect(view.banner).toBe("analysis may be compromised");
    expect(view.runAffordance).toBe("type-confirm");
    // The (possibly poisoned) verdict is still carried through, show-but-banner.
    expect(view.verdict).toBe(
      "Ollama LLM runtime. Standard vendor installer from the official domain.",
    );
    expect(view.behaviors).toEqual(BEHAVIORS);
  });

  test("ok caution + manipulation FAILED: failed is treated as not-clean (allowlist clean; never denylist fired)", () => {
    // `failed` is neither clean nor fired — it must still fall to the
    // manipulation state, proving the rule is an allowlist of `clean`.
    const view = deriveInsightView(ok("caution", { kind: "failed", reason: "timeout" }), URL);

    expect(view.state).toBe("manipulation");
    if (view.state !== "manipulation") return;
    expect(view.banner).toBe("analysis may be compromised");
    expect(view.runAffordance).toBe("type-confirm");
  });

  test("analysis FAILED + manipulation clean: analysis-failed state, message with trailing period, button", () => {
    const result: AnalysisResult = {
      kind: "analyzed",
      analysis: { kind: "failed", reason: "model exploded" },
      manipulation: { kind: "clean" },
    };

    const view = deriveInsightView(result, URL);

    expect(view.state).toBe("analysis-failed");
    if (view.state !== "analysis-failed") return;
    expect(view.message).toBe("Couldn't analyze: model exploded.");
    expect(view.runAffordance).toBe("button");
  });

  test("analysis FAILED + manipulation FIRED: analysis-failed WINS over the manipulation banner (key precedence)", () => {
    // A "may be compromised" banner over a "couldn't analyze" body is incoherent:
    // there is no verdict to caveat, so analysis-failed must take over even
    // though the manipulation pass fired.
    const result: AnalysisResult = {
      kind: "analyzed",
      analysis: { kind: "failed", reason: "provider outage" },
      manipulation: { kind: "fired" },
    };

    const view = deriveInsightView(result, URL);

    expect(view.state).toBe("analysis-failed");
    if (view.state !== "analysis-failed") return;
    expect(view.message).toBe("Couldn't analyze: provider outage.");
    expect(view.runAffordance).toBe("button");
  });
});

describe("deriveInsightView — source display", () => {
  // Driven through a noProvider result so we exercise the real entry point
  // rather than an exported formatter.
  const sourceOf = (url: string): string => deriveInsightView({ kind: "noProvider" }, url).source;

  test("a normal https URL becomes host + path with no scheme", () => {
    expect(sourceOf("https://ollama.com/install.sh")).toBe("ollama.com/install.sh");
  });

  test("a lone trailing slash is trimmed", () => {
    expect(sourceOf("https://get.docker.com/")).toBe("get.docker.com");
  });

  test("a malformed URL falls back to the raw string unchanged", () => {
    expect(sourceOf("not a url")).toBe("not a url");
  });
});
