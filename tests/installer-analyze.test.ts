/**
 * `analyzeScript` — the two-pass LLM analysis core (step 2 of the LLM-insights
 * dialog feature). Two concurrent passes (analysis + manipulation), each its
 * own conversation/handle, never throws, returns a structured `AnalysisResult`
 * the dialog view-model consumes.
 *
 * The two-pass test contract: `SWEEP_TEST_RESPONSES` is interpreted as a JSON
 * object `{ analysis: <TestResponses>, manipulation: <TestResponses> }`. Each
 * key feeds its own `createLlm({name:"test"})` handle, so the two passes are
 * addressed independently (each cursor starts at 0). The preload wipes the var
 * before every test.
 *
 * Fixture scripts are harmless if executed (`echo`, `true`) per testing.md —
 * analyzeScript never execs, but the rule holds across the suite.
 */

import { describe, expect, test } from "bun:test";
import { analyzeScript } from "../src/installer/analyze.ts";

const bytes = (s: string): Uint8Array => new TextEncoder().encode(s);
const SCRIPT = bytes("echo hello\ntrue\n");
const URL = "https://example.com/install.sh";

/** Set the two-pass env contract for one test. */
function setResponses(analysis: unknown, manipulation: unknown): void {
  process.env.SWEEP_TEST_RESPONSES = JSON.stringify({ analysis, manipulation });
}

describe("analyzeScript — two-pass core", () => {
  test("no provider seam: the result is a single noProvider without invoking a provider", async () => {
    // The preload deleted SWEEP_TEST_RESPONSES. With no provider config and no
    // test seam, analyzeScript must short-circuit to the whole-result noProvider
    // discriminator (neither pass runs) and never throw.
    const result = await analyzeScript({ url: URL, scriptBytes: SCRIPT });

    expect(result).toEqual({ kind: "noProvider" });
  });

  test("ok analysis + clean manipulation: parsed fields flow through to the result", async () => {
    // Also pins per-pass isolation: analysis is fed an object and manipulation a
    // boolean, and BOTH resolve to their own value — impossible if the two
    // handles shared a response cursor (one would consume the other's reply).
    setResponses(
      {
        severity: "caution",
        verdict: "Installs a CLI to /usr/local/bin and edits PATH in your shell rc.",
        flags: ["Requests sudo", "Edits ~/.bashrc"],
        behaviors: [
          { description: "Install binary to /usr/local/bin", sudo: true },
          { description: "Append export line to ~/.bashrc", sudo: false },
        ],
      },
      { manipulationDetected: false },
    );

    const result = await analyzeScript({ url: URL, scriptBytes: SCRIPT });

    expect(result).toEqual({
      kind: "analyzed",
      analysis: {
        kind: "ok",
        severity: "caution",
        verdict: "Installs a CLI to /usr/local/bin and edits PATH in your shell rc.",
        flags: ["Requests sudo", "Edits ~/.bashrc"],
        behaviors: [
          { description: "Install binary to /usr/local/bin", sudo: true },
          { description: "Append export line to ~/.bashrc", sudo: false },
        ],
      },
      manipulation: { kind: "clean" },
    });
  });

  test("manipulation fired: detected boolean maps to fired, independent of a clean analysis pass", async () => {
    setResponses(
      {
        severity: "clear",
        verdict: "Looks like a standard vendor installer.",
        flags: [],
        behaviors: [{ description: "Download a binary", sudo: false }],
      },
      { manipulationDetected: true },
    );

    const result = await analyzeScript({ url: URL, scriptBytes: SCRIPT });

    expect(result.kind).toBe("analyzed");
    if (result.kind !== "analyzed") return;
    expect(result.manipulation).toEqual({ kind: "fired" });
    expect(result.analysis).toMatchObject({ kind: "ok", severity: "clear" });
  });

  test("analysis provider error: that pass fails with the bare message; manipulation is unaffected", async () => {
    setResponses(["ERROR: model exploded"], { manipulationDetected: false });

    const result = await analyzeScript({ url: URL, scriptBytes: SCRIPT });

    expect(result.kind).toBe("analyzed");
    if (result.kind !== "analyzed") return;
    expect(result.analysis).toEqual({ kind: "failed", reason: "model exploded" });
    expect(result.manipulation).toEqual({ kind: "clean" });
  });

  test("analysis schema mismatch: valid JSON that fails the schema fails the analysis pass; install-agnostic", async () => {
    // The reply is valid JSON (`{"wrong":"shape"}`) but does not match the
    // analysis schema. Both the send and its one parse retry consume the
    // repeating response and fail schema validation, so the pass throws
    // LlmParseError, which analyzeScript catches as failed. (An object fixture,
    // not a non-JSON string, is what exercises the schema-mismatch branch rather
    // than the JSON-parse-fail branch.)
    setResponses({ wrong: "shape" }, { manipulationDetected: false });

    const result = await analyzeScript({ url: URL, scriptBytes: SCRIPT });

    expect(result.kind).toBe("analyzed");
    if (result.kind !== "analyzed") return;
    expect(result.analysis.kind).toBe("failed");
    expect(result.manipulation).toEqual({ kind: "clean" });
  });

  test("abort: an already-aborted signal fails both passes within an analyzed result and never throws", async () => {
    setResponses(
      { severity: "clear", verdict: "ok", flags: [], behaviors: [] },
      { manipulationDetected: false },
    );

    const result = await analyzeScript({
      url: URL,
      scriptBytes: SCRIPT,
      signal: AbortSignal.abort(),
    });

    // The seam exists (env set), so this is an attempted analysis, not
    // noProvider; the aborted signal maps each pass to failed.
    expect(result.kind).toBe("analyzed");
    if (result.kind !== "analyzed") return;
    expect(result.analysis.kind).toBe("failed");
    expect(result.manipulation.kind).toBe("failed");
  });

  test("malformed two-pass contract: a present-but-broken seam is analyzed/failed, not noProvider", async () => {
    // The env value is valid JSON but not the {analysis, manipulation} object
    // shape. The seam EXISTS but is broken, so this is an attempted-but-failed
    // analysis (analyzed + both passes failed), NOT noProvider. Never throws.
    process.env.SWEEP_TEST_RESPONSES = JSON.stringify(["just an array"]);

    const result = await analyzeScript({ url: URL, scriptBytes: SCRIPT });

    expect(result.kind).toBe("analyzed");
    if (result.kind !== "analyzed") return;
    expect(result.analysis.kind).toBe("failed");
    expect(result.manipulation.kind).toBe("failed");
  });
});
