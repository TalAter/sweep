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

import { afterEach, describe, expect, test } from "bun:test";
import { createLlm } from "wrap-core/llm";
import { analyzeScript, resolveAnalysisProvider } from "../src/installer/analyze.ts";

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

  test("empty test-response array: an unplayable seam is analyzed/failed, not a throw", async () => {
    // An empty array is the {analysis,manipulation} shape but unplayable —
    // createLlm({name:"test"}) rejects it. analyzeScript builds test handles
    // outside a try, so this must be classified broken upstream rather than
    // throwing out of the never-throws contract.
    process.env.SWEEP_TEST_RESPONSES = JSON.stringify({
      analysis: [],
      manipulation: { manipulationDetected: false },
    });

    const result = await analyzeScript({ url: URL, scriptBytes: SCRIPT });

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

  // The `{kind:"real"}` branch — both passes sharing ONE resolved `Llm` handle
  // across two conversations, sends wrapped in the ANALYSIS_TIMEOUT_MS combiner
  // — is otherwise reachable only via a live network provider. These two tests
  // inject a test-backed real handle through analyzeScript's second param so the
  // real branch runs deterministically with NO network.
  //
  // A SHARED handle = ONE adapter = ONE response cursor: the two concurrent
  // passes draw from a single ordered list (unlike the `test` provider, which
  // builds a fresh handle per pass). Pass order over that one cursor isn't
  // deterministic, so both entries are crafted to satisfy BOTH schemas and carry
  // the SAME outcome — whichever pass reads whichever entry, the result is fixed.
  // The 2-entry array drained by exactly two passes is the shared-cursor proof.
  test("real provider injected: two passes share one handle off a single ordered cursor → analyzed", async () => {
    // Each entry is a dual-schema object (Zod strips unknown keys): the analysis
    // pass reads severity/verdict/flags/behaviors, the manipulation pass reads
    // manipulationDetected — from the same object, so order can't change either
    // outcome. Reaching {kind:"real"} (not test/none/broken) is what this pins.
    const dualEntry = {
      severity: "caution",
      verdict: "Installs a CLI and requests sudo.",
      flags: ["Requests sudo"],
      behaviors: [{ description: "Install binary to /usr/local/bin", sudo: true }],
      manipulationDetected: true,
    };
    const llm = createLlm({ name: "test", responses: [dualEntry, dualEntry] });

    const result = await analyzeScript({ url: URL, scriptBytes: SCRIPT }, { kind: "real", llm });

    expect(result).toEqual({
      kind: "analyzed",
      analysis: {
        kind: "ok",
        severity: "caution",
        verdict: "Installs a CLI and requests sudo.",
        flags: ["Requests sudo"],
        behaviors: [{ description: "Install binary to /usr/local/bin", sudo: true }],
      },
      manipulation: { kind: "fired" },
    });
  });

  test("real provider injected: a live-call failure on each pass → both passes failed, still analyzed", async () => {
    // An `ERROR:`-prefixed test response makes the provider's call() throw — a
    // RESOLVED provider whose live call fails. That is a failed PASS (the script
    // was analyzed, the call just failed), never noProvider/broken. Both entries
    // error so the outcome is order-independent across the shared cursor.
    const llm = createLlm({ name: "test", responses: ["ERROR: model exploded", "ERROR: model exploded"] });

    const result = await analyzeScript({ url: URL, scriptBytes: SCRIPT }, { kind: "real", llm });

    expect(result.kind).toBe("analyzed");
    if (result.kind !== "analyzed") return;
    expect(result.analysis).toEqual({ kind: "failed", reason: "model exploded" });
    expect(result.manipulation).toEqual({ kind: "failed", reason: "model exploded" });
  });
});

/**
 * `resolveAnalysisProvider` — the never-throws provider-selection function
 * `analyzeScript` dispatches on. Test seam first (preserving today's absent vs
 * present-but-broken split), else config-resolved real provider, else
 * `{kind:"none"}` for ANY resolution/config-time failure. These cases need no
 * network: `createLlm` validates eagerly but does not call until `send`.
 *
 * Both env reads (`SWEEP_TEST_RESPONSES` for the seam, `SWEEP_CONFIG` for the
 * real provider) go through `process.env` uniformly — the function takes no
 * `env` param. Isolation: the SWEEP_HOME preload wipes SWEEP_TEST_RESPONSES
 * per test; afterEach below wipes both vars so nothing leaks between cases.
 */
describe("resolveAnalysisProvider", () => {
  afterEach(() => {
    delete process.env.SWEEP_CONFIG;
    delete process.env.SWEEP_TEST_RESPONSES;
  });

  test("test seam present + valid → {kind:'test'} carrying both pass response sets", () => {
    process.env.SWEEP_TEST_RESPONSES = JSON.stringify({
      analysis: { severity: "clear", verdict: "ok", flags: [], behaviors: [] },
      manipulation: { manipulationDetected: false },
    });
    const provider = resolveAnalysisProvider();
    expect(provider.kind).toBe("test");
    if (provider.kind !== "test") return;
    expect(provider.analysis).toEqual({
      severity: "clear",
      verdict: "ok",
      flags: [],
      behaviors: [],
    });
    expect(provider.manipulation).toEqual({ manipulationDetected: false });
  });

  test("test seam present but malformed → {kind:'broken'} with a bare reason", () => {
    process.env.SWEEP_TEST_RESPONSES = JSON.stringify(["just an array"]);
    const provider = resolveAnalysisProvider();
    expect(provider.kind).toBe("broken");
    if (provider.kind !== "broken") return;
    expect(provider.reason.length).toBeGreaterThan(0);
  });

  test("test seam with an empty response array → {kind:'broken'} (unplayable, not a throw)", () => {
    // Right shape, but an empty array is canned playback with nothing to play;
    // createLlm rejects it. Classified broken here so analyzeScript's test-handle
    // construction (outside a try) can never throw it.
    process.env.SWEEP_TEST_RESPONSES = JSON.stringify({
      analysis: [],
      manipulation: { manipulationDetected: false },
    });
    const provider = resolveAnalysisProvider();
    expect(provider.kind).toBe("broken");
    if (provider.kind !== "broken") return;
    expect(provider.reason.length).toBeGreaterThan(0);
  });

  // The real path runs `ensureConfig()`, which reads `process.env.SWEEP_CONFIG`
  // (the override `loadJsoncConfig` honors) — the same `process.env` the seam
  // read uses now that the function takes no `env` param. With the preload +
  // afterEach wiping SWEEP_TEST_RESPONSES, there is no seam, so these fall
  // through to the real-provider branch.
  test("no test seam + SWEEP_CONFIG with a valid provider → {kind:'real'} with a labeled llm", () => {
    // Anthropic + model + literal apiKey: createLlm validates and builds a
    // handle without any network call. SWEEP_CONFIG short-circuits the file read.
    process.env.SWEEP_CONFIG = JSON.stringify({
      defaultProvider: "anthropic",
      providers: { anthropic: { model: "claude-sonnet-4-6", apiKey: "sk-x" } },
    });
    const provider = resolveAnalysisProvider();
    expect(provider.kind).toBe("real");
    if (provider.kind !== "real") return;
    expect(provider.llm.label).toBe("anthropic / claude-sonnet-4-6");
  });

  test("no test seam + SWEEP_CONFIG with no defaultProvider → {kind:'none'}", () => {
    process.env.SWEEP_CONFIG = JSON.stringify({ providers: { anthropic: { model: "m" } } });
    expect(resolveAnalysisProvider()).toEqual({ kind: "none" });
  });

  test("no test seam + SWEEP_CONFIG naming an unknown provider missing fields → {kind:'none'}", () => {
    // Unknown provider requires baseURL+apiKey+model; this entry has none, so
    // resolution throws and is mapped to none (no usable provider).
    process.env.SWEEP_CONFIG = JSON.stringify({
      defaultProvider: "mystery",
      providers: { mystery: {} },
    });
    expect(resolveAnalysisProvider()).toEqual({ kind: "none" });
  });

  test("no test seam + SWEEP_CONFIG with $UNSET_VAR apiKey → {kind:'none'} (createLlm throws)", () => {
    delete process.env.SWEEP_NO_SUCH_KEY_ANALYZE_TEST;
    process.env.SWEEP_CONFIG = JSON.stringify({
      defaultProvider: "anthropic",
      providers: {
        anthropic: { model: "claude-sonnet-4-6", apiKey: "$SWEEP_NO_SUCH_KEY_ANALYZE_TEST" },
      },
    });
    expect(resolveAnalysisProvider()).toEqual({ kind: "none" });
  });

  test("no test seam + malformed SWEEP_CONFIG JSON → {kind:'none'} (config throws, mapped)", () => {
    process.env.SWEEP_CONFIG = "{ not json";
    expect(resolveAnalysisProvider()).toEqual({ kind: "none" });
  });

  test("nothing set → {kind:'none'}", () => {
    expect(resolveAnalysisProvider()).toEqual({ kind: "none" });
  });

  test("blank test seam (empty/whitespace) counts as absent, not a broken seam → {kind:'none'}", () => {
    // `SWEEP_TEST_RESPONSES=` neutralizes an inherited var; it must NOT be a
    // broken seam (which would render an analysis-failed dialog). With no real
    // provider configured this falls through to none. Whitespace-only too.
    process.env.SWEEP_TEST_RESPONSES = "";
    expect(resolveAnalysisProvider()).toEqual({ kind: "none" });
    process.env.SWEEP_TEST_RESPONSES = "   ";
    expect(resolveAnalysisProvider()).toEqual({ kind: "none" });
  });
});
