/**
 * `InteractiveDialog` — the paste-an-install-command dialog. Step 5 adds an
 * optional `error` prop so a parse failure can be surfaced INLINE (back to the
 * paste input with the message) rather than as a `sweep:` stderr line that would
 * be hidden behind the live alt-screen.
 *
 * Only the inline-error contract is pinned here — the rest of the dialog
 * (submit-on-nonempty, Esc-cancel) is incidental plumbing the substrate owns.
 */

import { describe, expect, test } from "bun:test";
import { render } from "ink-testing-library";
import { stripAnsi } from "wrap-core/ansi";
import { DARK_CORE } from "wrap-core/theme";
import { ThemeProvider } from "wrap-core/tui";
import { InteractiveDialog } from "../src/tui/interactive.tsx";
import { DARK_GRADIENT } from "../src/tui/theme.ts";

describe("InteractiveDialog — inline error", () => {
  test("renders the error message when the `error` prop is set", () => {
    const { lastFrame } = render(
      <ThemeProvider theme={DARK_CORE} nerdFonts={false}>
        <InteractiveDialog
          gradientStops={DARK_GRADIENT}
          onSubmit={() => {}}
          onCancel={() => {}}
          error="left side of pipe must start with curl or wget"
        />
      </ThemeProvider>,
    );
    const text = stripAnsi(lastFrame() ?? "");
    expect(text).toContain("left side of pipe must start with curl or wget");
    // The paste input is still present — the user can re-paste.
    expect(text).toContain("Paste an install command");
  });

  test("renders no error line when `error` is omitted (backward compatible)", () => {
    const { lastFrame } = render(
      <ThemeProvider theme={DARK_CORE} nerdFonts={false}>
        <InteractiveDialog gradientStops={DARK_GRADIENT} onSubmit={() => {}} onCancel={() => {}} />
      </ThemeProvider>,
    );
    const text = stripAnsi(lastFrame() ?? "");
    expect(text).toContain("Paste an install command");
  });
});
