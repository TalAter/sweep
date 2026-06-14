/**
 * `runInstallSession` — the step-5 session controller. It owns ONE alt-screen
 * session driving paste → loading → resolved, runs fetch+analyze beneath it, and
 * resolves to an `InstallDecision`. These tests inject the mount seam (backed by
 * ink-testing-library) and controllable fetch/analyze so no real terminal,
 * network, or LLM is touched.
 *
 * Contracts pinned (each a sentence):
 *  - direct mode starts at the loading spinner, then shows the resolved view
 *  - interactive bad paste returns to the paste input with the parse error (no advance)
 *  - interactive good paste advances paste → loading → resolved
 *  - run from resolved settles {kind:"run"} carrying the fetched bytes/sha
 *  - cancel during loading settles {kind:"cancel", fetched:null} and aborts fetch
 *  - cancel after fetch / during analyze settles cancel with fetched present + aborts analyze
 *  - cancel WINS over the abort-induced analysis-failed view (never flashes "Couldn't analyze")
 *  - a fetch throw settles {kind:"fetch-failed"} carrying the error
 */

import { describe, expect, test } from "bun:test";
import { render } from "ink-testing-library";
import type { ReactElement } from "react";
import { stripAnsi } from "wrap-core/ansi";
import { DARK_CORE } from "wrap-core/theme";
import { ThemeProvider } from "wrap-core/tui";
import type { AnalysisResult } from "../src/installer/analyze.ts";
import { type FetchedScript, FetchScriptError } from "../src/installer/fetch.ts";
import { parseInstallCommand } from "../src/installer/parse.ts";
import { type InstallDecision, runInstallSession } from "../src/tui/install-session.ts";
import { DARK_GRADIENT } from "../src/tui/theme.ts";
import { waitFor } from "./helpers.ts";

// A harmless install command — `true` as the body if anything ever ran it.
const RAW = "curl https://example.com/install.sh | sh";
const PARSED = parseInstallCommand(RAW);
if ("kind" in PARSED) throw new Error("test fixture failed to parse");

const FETCHED: FetchedScript = {
  bytes: new TextEncoder().encode("#!/bin/sh\ntrue\n"),
  sha256: "deadbeef",
  finalUrl: "https://example.com/install.sh",
  fetchedAt: "2026-06-14T00:00:00.000Z",
  status: 200,
};

const CLEAR_RESULT: AnalysisResult = {
  kind: "analyzed",
  analysis: {
    kind: "ok",
    severity: "clear",
    verdict: "Foo CLI. Standard vendor installer.",
    flags: [],
    behaviors: [{ description: "Download the foo binary", sudo: false }],
  },
  manipulation: { kind: "clean" },
};

/** A deferred promise plus its resolve/reject — lets a test drive async timing. */
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/**
 * A mount seam backed by ink-testing-library. We capture the latest render
 * result so the test can read `lastFrame()` and drive `stdin`, AND we record
 * EVERY element ever mounted/rerendered. The element log is timing-independent:
 * cancel-wins can assert the controller never asked to render the resolved view,
 * which is stronger than racing against ink's frame buffer.
 */
function makeMountSeam() {
  let app: ReturnType<typeof render> | null = null;
  const rendered: ReactElement[] = [];
  const wrap = (el: ReactElement) => (
    <ThemeProvider theme={DARK_CORE} nerdFonts={false}>
      {el}
    </ThemeProvider>
  );

  const mount = (el: ReactElement) => {
    rendered.push(el);
    app = render(wrap(el));
    return {
      rerender(next: ReactElement) {
        rendered.push(next);
        app?.rerender(wrap(next));
      },
      unmount() {
        app?.unmount();
      },
    };
  };

  return {
    mount,
    frame: () => stripAnsi(app?.lastFrame() ?? ""),
    write: (s: string) => app?.stdin.write(s),
    /** True if the controller ever rendered an InsightDialog in the resolved phase. */
    everRenderedResolved: () =>
      rendered.some(
        (el) => (el.props as { state?: { phase?: string } }).state?.phase === "resolved",
      ),
  };
}

const sessionOpts = (
  seam: ReturnType<typeof makeMountSeam>,
  start: { kind: "interactive" } | { kind: "direct"; raw: string; parsed: typeof PARSED },
  deps: {
    fetchScript?: (url: string, o?: { signal?: AbortSignal }) => Promise<FetchedScript>;
    analyzeScript?: (a: {
      url: string;
      scriptBytes: Uint8Array;
      signal?: AbortSignal;
    }) => Promise<AnalysisResult>;
  },
) => ({
  start,
  gradientStops: DARK_GRADIENT,
  theme: DARK_CORE,
  nerdFonts: false,
  deps: { mount: seam.mount, ...deps },
});

describe("runInstallSession — direct mode", () => {
  test("starts at the loading spinner, then shows the resolved view after fetch+analyze", async () => {
    const seam = makeMountSeam();
    const analyzeGate = deferred<AnalysisResult>();

    const decisionP = runInstallSession(
      sessionOpts(
        seam,
        { kind: "direct", raw: RAW, parsed: PARSED },
        {
          fetchScript: async () => FETCHED,
          analyzeScript: () => analyzeGate.promise,
        },
      ),
    );

    // Loading first: the spinner line is up before analysis resolves.
    await waitFor(() => expect(seam.frame()).toContain("Analyzing"));
    expect(seam.frame()).toContain("example.com/install.sh");

    analyzeGate.resolve(CLEAR_RESULT);

    await waitFor(() => expect(seam.frame()).toContain("Foo CLI. Standard vendor installer."));
    // Resolved view is up; the run affordance (a button) is present.
    expect(seam.frame()).toContain("Run");

    // Cancel to settle the session and end the test cleanly.
    seam.write("");
    await decisionP;
  });
});

describe("runInstallSession — interactive mode", () => {
  test("bad paste: the parse error appears and the session stays on the paste input", async () => {
    const seam = makeMountSeam();
    let fetchCalls = 0;

    const decisionP = runInstallSession(
      sessionOpts(
        seam,
        { kind: "interactive" },
        {
          fetchScript: async () => {
            fetchCalls++;
            return FETCHED;
          },
          analyzeScript: async () => CLEAR_RESULT,
        },
      ),
    );

    await waitFor(() => expect(seam.frame()).toContain("Paste an install command"));

    // A command with no pipe fails parse (no-pipe / unsupported).
    seam.write("curl https://example.com/install.sh");
    await waitFor(() => expect(seam.frame()).toContain("install.sh"));
    seam.write("\r");

    // The parse error surfaces inline and we stay on the paste input — no advance
    // to loading, fetch never called. (Asserts a fragment of the no-pipe message
    // short enough not to soft-wrap at the dialog width.)
    await waitFor(() => expect(seam.frame()).toContain("found no"));
    expect(seam.frame()).toContain("Paste an install command");
    expect(seam.frame()).not.toContain("Analyzing");
    expect(fetchCalls).toBe(0);

    seam.write("");
    const decision = await decisionP;
    expect(decision.kind).toBe("cancel");
  });

  test("good paste: a valid command advances paste → loading → resolved", async () => {
    const seam = makeMountSeam();
    const analyzeGate = deferred<AnalysisResult>();

    const decisionP = runInstallSession(
      sessionOpts(
        seam,
        { kind: "interactive" },
        {
          fetchScript: async () => FETCHED,
          analyzeScript: () => analyzeGate.promise,
        },
      ),
    );

    await waitFor(() => expect(seam.frame()).toContain("Paste an install command"));
    seam.write(RAW);
    await waitFor(() => expect(seam.frame()).toContain("install.sh"));
    seam.write("\r");

    await waitFor(() => expect(seam.frame()).toContain("Analyzing"));
    analyzeGate.resolve(CLEAR_RESULT);
    await waitFor(() => expect(seam.frame()).toContain("Foo CLI. Standard vendor installer."));

    seam.write("");
    await decisionP;
  });
});

describe("runInstallSession — run", () => {
  test("from a clear (button) resolved view, focus Run + Enter settles {kind:'run'} carrying the fetch", async () => {
    const seam = makeMountSeam();

    const decisionP = runInstallSession(
      sessionOpts(
        seam,
        { kind: "direct", raw: RAW, parsed: PARSED },
        {
          fetchScript: async () => FETCHED,
          analyzeScript: async () => CLEAR_RESULT,
        },
      ),
    );

    await waitFor(() => expect(seam.frame()).toContain("Run"));
    // Move focus to Run, then Enter.
    seam.write("[C"); // right arrow
    await new Promise((r) => setTimeout(r, 30));
    seam.write("\r");

    const decision = await decisionP;
    expect(decision.kind).toBe("run");
    if (decision.kind !== "run") return;
    expect(decision.fetched.sha256).toBe("deadbeef");
    expect(decision.raw).toBe(RAW);
  });
});

describe("runInstallSession — cancel", () => {
  test("during loading: Esc before fetch resolves settles cancel with fetched:null and aborts fetch", async () => {
    const seam = makeMountSeam();
    const fetchGate = deferred<FetchedScript>();
    let capturedSignal: AbortSignal | undefined;

    const decisionP = runInstallSession(
      sessionOpts(
        seam,
        { kind: "direct", raw: RAW, parsed: PARSED },
        {
          fetchScript: (_url, o) => {
            capturedSignal = o?.signal;
            return fetchGate.promise;
          },
          analyzeScript: async () => CLEAR_RESULT,
        },
      ),
    );

    await waitFor(() => expect(seam.frame()).toContain("Analyzing"));
    seam.write(""); // Esc

    const decision = await decisionP;
    expect(decision.kind).toBe("cancel");
    if (decision.kind !== "cancel") return;
    expect(decision.fetched).toBeNull();
    expect(capturedSignal?.aborted).toBe(true);
  });

  test("after fetch, during analyze: cancel settles with fetched present and aborts analyze", async () => {
    const seam = makeMountSeam();
    const analyzeGate = deferred<AnalysisResult>();
    let capturedSignal: AbortSignal | undefined;

    const decisionP = runInstallSession(
      sessionOpts(
        seam,
        { kind: "direct", raw: RAW, parsed: PARSED },
        {
          fetchScript: async () => FETCHED,
          analyzeScript: (a) => {
            capturedSignal = a.signal;
            return analyzeGate.promise;
          },
        },
      ),
    );

    // Wait until analyze has started (its signal was captured).
    await waitFor(() => expect(capturedSignal).toBeDefined());
    seam.write(""); // Esc

    const decision = await decisionP;
    expect(decision.kind).toBe("cancel");
    if (decision.kind !== "cancel") return;
    expect(decision.fetched).not.toBeNull();
    expect(decision.fetched?.sha256).toBe("deadbeef");
    expect(capturedSignal?.aborted).toBe(true);
  });

  test("cancel WINS: aborting analyze maps to analysis-failed inside analyzeScript, but the session never flashes 'Couldn't analyze'", async () => {
    const seam = makeMountSeam();
    let capturedSignal: AbortSignal | undefined;

    // Mirror analyzeScript's real behavior: an aborted signal resolves to an
    // analyzed result with a failed analysis pass (it never throws).
    const analyzeScript = (a: { signal?: AbortSignal }): Promise<AnalysisResult> =>
      new Promise((resolve) => {
        capturedSignal = a.signal;
        a.signal?.addEventListener(
          "abort",
          () =>
            resolve({
              kind: "analyzed",
              analysis: { kind: "failed", reason: "aborted" },
              manipulation: { kind: "failed", reason: "aborted" },
            }),
          { once: true },
        );
      });

    const decisionP = runInstallSession(
      sessionOpts(
        seam,
        { kind: "direct", raw: RAW, parsed: PARSED },
        { fetchScript: async () => FETCHED, analyzeScript },
      ),
    );

    await waitFor(() => expect(capturedSignal).toBeDefined());
    seam.write(""); // Esc -> abort -> analyzeScript resolves to analysis-failed

    const decision = await decisionP;

    expect(decision.kind).toBe("cancel");
    // The controller must short-circuit BEFORE rendering the abort-as-failed
    // view - it never even asks for the resolved phase (timing-independent).
    expect(seam.everRenderedResolved()).toBe(false);
  });
});

describe("runInstallSession — fetch failure", () => {
  test("a fetch throw settles {kind:'fetch-failed'} carrying the error", async () => {
    const seam = makeMountSeam();
    const err = new FetchScriptError(
      "non-2xx",
      "HTTP 404 Not Found: https://example.com/install.sh",
    );

    const decision: InstallDecision = await runInstallSession(
      sessionOpts(
        seam,
        { kind: "direct", raw: RAW, parsed: PARSED },
        {
          fetchScript: async () => {
            throw err;
          },
          analyzeScript: async () => CLEAR_RESULT,
        },
      ),
    );

    expect(decision.kind).toBe("fetch-failed");
    if (decision.kind !== "fetch-failed") return;
    expect(decision.error).toBe(err);
    expect(decision.raw).toBe(RAW);
  });
});
