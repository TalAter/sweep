import { beforeEach, describe, expect, mock, test } from "bun:test";
import { render } from "ink-testing-library";
import { stripAnsi } from "wrap-core/ansi";
import { DARK_CORE, setTheme } from "wrap-core/theme";
import { ThemeProvider } from "wrap-core/tui";
import type { Behavior } from "../src/installer/analyze.ts";
import { InsightDialog, type InsightDialogState } from "../src/tui/insight-dialog.tsx";
import type { InsightView } from "../src/tui/insight-view.ts";
import { DARK_GRADIENT } from "../src/tui/theme.ts";
import { waitFor } from "./helpers.ts";

// Harmless behaviors — these are display strings only, but the testing rules say
// fixtures must be safe even if a regression somehow executed them.
const SAFE_BEHAVIORS: Behavior[] = [
  { description: "Download the foo binary from example.com", sudo: false },
  { description: "Install to /usr/local/bin/foo", sudo: true },
];

function renderDialog(
  state: InsightDialogState,
  handlers?: { onRun?: () => void; onCancel?: () => void },
) {
  return render(
    <ThemeProvider theme={DARK_CORE} nerdFonts={false}>
      <InsightDialog
        state={state}
        neutralGradient={DARK_GRADIENT}
        onRun={handlers?.onRun ?? (() => {})}
        onCancel={handlers?.onCancel ?? (() => {})}
      />
    </ThemeProvider>,
  );
}

function resolved(view: InsightView): InsightDialogState {
  return { phase: "resolved", view };
}

// Widest visible span in a rendered frame = the dialog's border width. The outer
// centering box pads each line with spaces; trimming leading/trailing space leaves
// the dialog's own width (the border line spans corner-to-corner = totalWidth).
function frameWidth(frame: string): number {
  let max = 0;
  for (const raw of stripAnsi(frame).split("\n")) {
    const line = raw.replace(/\s+$/, "");
    const start = line.search(/\S/);
    if (start === -1) continue;
    max = Math.max(max, line.length - start);
  }
  return max;
}

beforeEach(() => {
  setTheme(DARK_CORE);
});

describe("InsightDialog — content per state", () => {
  test("clear: shows source, verdict, behaviors with (not exhaustive) and a sudo marker, no severity pill, and a Run action", () => {
    const { lastFrame } = renderDialog(
      resolved({
        state: "clear",
        source: "example.com/install.sh",
        verdict: "Foo CLI. Standard vendor installer.",
        flags: [],
        behaviors: SAFE_BEHAVIORS,
        runAffordance: "button",
      }),
    );
    const text = stripAnsi(lastFrame() ?? "");
    expect(text).toContain("example.com/install.sh");
    expect(text).toContain("Foo CLI. Standard vendor installer.");
    expect(text).toContain("(not exhaustive)");
    expect(text).toContain("Install to /usr/local/bin/foo");
    expect(text).toContain("(sudo)");
    expect(text).toContain("Run");
    expect(text).not.toContain("caution");
    expect(text).not.toContain("danger");
  });

  test("caution: shows the ⚠ caution pill, a Flags section, and a Run action", () => {
    const { lastFrame } = renderDialog(
      resolved({
        state: "caution",
        source: "example.com/install.sh",
        verdict: "Installs a service.",
        flags: ["Enables a background service"],
        behaviors: SAFE_BEHAVIORS,
        runAffordance: "button",
      }),
    );
    const text = stripAnsi(lastFrame() ?? "");
    expect(text).toContain("⚠ caution");
    expect(text).toContain("Flags:");
    expect(text).toContain("Enables a background service");
    expect(text).toContain("Run");
  });

  test("danger: shows the ✗ danger pill and the type-'install' input, no plain Run button", () => {
    const { lastFrame } = renderDialog(
      resolved({
        state: "danger",
        source: "evil.example/install.sh",
        verdict: "Treat with suspicion.",
        flags: ["Downloads from a raw IP"],
        behaviors: SAFE_BEHAVIORS,
        runAffordance: "type-confirm",
      }),
    );
    const text = stripAnsi(lastFrame() ?? "");
    expect(text).toContain("✗ danger");
    expect(text).toContain("Type 'install' to run");
    // No plain Run button (the affordance is the type-to-confirm input).
    expect(text).not.toMatch(/\bRun\b/);
  });

  test("manipulation: shows the compromise banner and the type-'install' input, no severity pill", () => {
    const { lastFrame } = renderDialog(
      resolved({
        state: "manipulation",
        source: "example.com/install.sh",
        verdict: "Claims to be safe.",
        flags: ["Contains prompt-injection text"],
        behaviors: SAFE_BEHAVIORS,
        banner: "analysis may be compromised",
        runAffordance: "type-confirm",
      }),
    );
    const text = stripAnsi(lastFrame() ?? "");
    expect(text).toContain("analysis may be compromised");
    expect(text).toContain("Type 'install' to run");
    expect(text).not.toContain("✗ danger");
    expect(text).not.toContain("⚠ caution");
  });

  test("no-llm: shows the no-provider message and a Run action", () => {
    const { lastFrame } = renderDialog(
      resolved({
        state: "no-llm",
        source: "example.com/install.sh",
        message: "No LLM provider configured — no analysis to show.",
        runAffordance: "button",
      }),
    );
    const text = stripAnsi(lastFrame() ?? "");
    expect(text).toContain("No LLM provider configured");
    expect(text).toContain("Run");
  });

  test("analysis-failed: shows the Couldn't analyze message and a Run action", () => {
    const { lastFrame } = renderDialog(
      resolved({
        state: "analysis-failed",
        source: "example.com/install.sh",
        message: "Couldn't analyze: provider timed out.",
        runAffordance: "button",
      }),
    );
    const text = stripAnsi(lastFrame() ?? "");
    expect(text).toContain("Couldn't analyze:");
    expect(text).toContain("Run");
  });

  test("loading: shows the Analyzing spinner line, the source, and no Run action", () => {
    const { lastFrame } = renderDialog({ phase: "loading", source: "example.com/install.sh" });
    const text = stripAnsi(lastFrame() ?? "");
    expect(text).toContain("example.com/install.sh");
    expect(text).toContain("Analyzing");
    expect(text).not.toMatch(/\bRun\b/);
  });
});

describe("InsightDialog — width", () => {
  test("loading stays compact — short content doesn't blow the box out to full width", () => {
    const { lastFrame } = renderDialog({ phase: "loading", source: "example.com/install.sh" });
    expect(frameWidth(lastFrame() ?? "")).toBeLessThanOrEqual(60);
  });

  test("grows to ~terminal width so long analysis prose wraps wide, not into a narrow tall column", () => {
    const { lastFrame } = renderDialog(
      resolved({
        state: "clear",
        source: "example.com/install.sh",
        verdict:
          "Standard vendor installer that downloads a release tarball, verifies its checksum, and drops the binary into a directory on PATH.",
        flags: [],
        behaviors: SAFE_BEHAVIORS,
        runAffordance: "button",
      }),
    );
    // ink-testing-library renders at 100 columns; the dialog should fill it
    // (minus margin/border), not sit at the old fixed ~58.
    expect(frameWidth(lastFrame() ?? "")).toBeGreaterThanOrEqual(90);
  });
});

describe("InsightDialog — interaction", () => {
  test("danger: typing 'install' then Enter calls onRun", async () => {
    const onRun = mock(() => {});
    const { stdin, lastFrame } = renderDialog(
      resolved({
        state: "danger",
        source: "evil.example/install.sh",
        verdict: "Treat with suspicion.",
        flags: [],
        behaviors: [],
        runAffordance: "type-confirm",
      }),
      { onRun },
    );
    stdin.write("install");
    // Let the input value commit (and re-render) before sending Enter — the submit
    // handler reads the typed value, so it must have landed first.
    await waitFor(() => expect(stripAnsi(lastFrame() ?? "")).toContain("install"));
    stdin.write("\r");
    await waitFor(() => expect(onRun).toHaveBeenCalledTimes(1));
  });

  test("danger: typing the wrong word then Enter does NOT call onRun", async () => {
    const onRun = mock(() => {});
    const { stdin, lastFrame } = renderDialog(
      resolved({
        state: "danger",
        source: "evil.example/install.sh",
        verdict: "Treat with suspicion.",
        flags: [],
        behaviors: [],
        runAffordance: "type-confirm",
      }),
      { onRun },
    );
    stdin.write("nope");
    // Wait for the wrong word to land before Enter, so the submit reads "nope"
    // (not an empty field) — otherwise the non-call would be for the wrong reason.
    await waitFor(() => expect(stripAnsi(lastFrame() ?? "")).toContain("nope"));
    stdin.write("\r");
    // Negative assertion: waitFor returns on first pass and can't prove a
    // non-call, so wait a bounded beat for any (incorrect) onRun to fire, then assert.
    await new Promise((r) => setTimeout(r, 30));
    expect(onRun).not.toHaveBeenCalled();
  });

  test("Esc calls onCancel", async () => {
    const onCancel = mock(() => {});
    const { stdin } = renderDialog(
      resolved({
        state: "clear",
        source: "example.com/install.sh",
        verdict: "Fine.",
        flags: [],
        behaviors: [],
        runAffordance: "button",
      }),
      { onCancel },
    );
    stdin.write("");
    await waitFor(() => expect(onCancel).toHaveBeenCalledTimes(1));
  });

  test("button affordance: Enter on the default focus (Cancel) calls onCancel, not onRun", async () => {
    const onRun = mock(() => {});
    const onCancel = mock(() => {});
    const { stdin } = renderDialog(
      resolved({
        state: "clear",
        source: "example.com/install.sh",
        verdict: "Fine.",
        flags: [],
        behaviors: [],
        runAffordance: "button",
      }),
      { onRun, onCancel },
    );
    stdin.write("\r");
    await waitFor(() => expect(onCancel).toHaveBeenCalledTimes(1));
    expect(onRun).not.toHaveBeenCalled();
  });

  test("button affordance: right-arrow moves focus to Run, then Enter calls onRun", async () => {
    const onRun = mock(() => {});
    const onCancel = mock(() => {});
    const { stdin } = renderDialog(
      resolved({
        state: "clear",
        source: "example.com/install.sh",
        verdict: "Fine.",
        flags: [],
        behaviors: [],
        runAffordance: "button",
      }),
      { onRun, onCancel },
    );
    stdin.write("[C"); // right arrow
    // Let the focus move commit (re-render) before Enter — the return handler
    // reads focusedIndex, and the focus change isn't visible in stripped text.
    await new Promise((r) => setTimeout(r, 30));
    stdin.write("\r");
    await waitFor(() => expect(onRun).toHaveBeenCalledTimes(1));
    expect(onCancel).not.toHaveBeenCalled();
  });
});
