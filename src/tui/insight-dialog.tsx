/**
 * The install-approval dialog (step 4 of the LLM-insights feature). Purely
 * presentational + interactive: it renders the `InsightView` produced by the
 * step-3 view-model plus a loading phase, and emits `onRun` / `onCancel`. It
 * does NOT fetch or analyze — the step-5 controller drives `state` through props
 * (loading → resolved) and owns the abort/teardown.
 *
 * Three contracts the renderer owns (chrome, not LLM output):
 *
 *   - Frame colour. Only `caution` / `danger` tint the frame with their severity
 *     stops; every other state (clear, no-llm, analysis-failed, MANIPULATION,
 *     loading) keeps the neutral gradient. Manipulation deliberately uses the
 *     neutral frame: the severity is untrusted there, so the banner carries the
 *     alarm, not the frame.
 *
 *   - The fixed markers — the `⚠` flag bullet, the `(not exhaustive)` behaviours
 *     label, the leading `⚠` on the compromise banner, the inline `(sudo)` — are
 *     added here, never by the model.
 *
 *   - Run friction. `runAffordance` (from the view-model) decides the action
 *     area: `"button"` → `[Cancel] [Run]` with focus defaulting to Cancel;
 *     `"type-confirm"` → `[Cancel]` plus a focused "type `install`" input. The
 *     input is the documented exception to focus-defaults-to-Cancel — it must
 *     hold focus to be typeable.
 */

import { Box, Text, useAnimation } from "ink";
import { useState } from "react";
import type { Color } from "wrap-core/ansi";
import { resolveColorHex } from "wrap-core/ansi";
import { SPINNER_FRAMES, SPINNER_INTERVAL } from "wrap-core/chrome";
import {
  ActionBar,
  type ActionItem,
  Dialog,
  Pill,
  TextInput,
  useKeyBindings,
  useNerdFonts,
  useTheme,
} from "wrap-core/tui";
import type { Behavior } from "../installer/analyze.ts";
import { CONFIRM_WORD, type InsightView } from "./insight-view.ts";
import { getSeverityPreset } from "./theme.ts";

export type InsightDialogState =
  | { phase: "loading"; source: string }
  | { phase: "resolved"; view: InsightView };

export type InsightDialogProps = {
  state: InsightDialogState;
  /** Neutral frame, appearance-resolved by the caller (DARK_GRADIENT / LIGHT_GRADIENT). */
  neutralGradient: Color[];
  onRun: () => void;
  onCancel: () => void;
};

const CONFIRM_PROMPT = `Type '${CONFIRM_WORD}' to run:`;
// Natural content width: wide enough for the longest fixed line — the
// `Appears to do (not exhaustive):` label and the `Type 'install' to run:` prompt.
const NATURAL_CONTENT_WIDTH = 54;

/** Frame stops: severity tint only for caution/danger; neutral for everything else. */
function frameStops(state: InsightDialogState, neutral: Color[]): Color[] {
  if (state.phase === "resolved") {
    const { view } = state;
    if (view.state === "caution" || view.state === "danger") {
      return getSeverityPreset(view.state).stops;
    }
  }
  return neutral;
}

function BehaviorList({ behaviors, color }: { behaviors: Behavior[]; color: string }) {
  if (behaviors.length === 0) return null;
  return (
    <Box flexDirection="column" marginTop={1}>
      <Text color={color}>Appears to do (not exhaustive):</Text>
      {behaviors.map((b) => (
        <Text key={b.description} color={color}>
          {" • "}
          {b.description}
          {b.sudo ? "  (sudo)" : ""}
        </Text>
      ))}
    </Box>
  );
}

function FlagList({ flags, color }: { flags: string[]; color: string }) {
  if (flags.length === 0) return null;
  return (
    <Box flexDirection="column" marginTop={1}>
      <Text color={color}>Flags:</Text>
      {flags.map((flag) => (
        <Text key={flag} color={color}>
          {" ⚠ "}
          {flag}
        </Text>
      ))}
    </Box>
  );
}

export function InsightDialog({ state, neutralGradient, onRun, onCancel }: InsightDialogProps) {
  const theme = useTheme();
  const nerdFonts = useNerdFonts();
  const bodyColor = resolveColorHex(theme.copy.body);
  const supportingColor = resolveColorHex(theme.copy.supporting);
  // The compromise banner borrows the danger pill's fg — it is the loudest token
  // we have, and the manipulation frame stays neutral, so the banner needs to carry
  // the alarm on its own. Routed through getSeverityPreset like every other
  // severity access in this file rather than reaching into theme.severity directly.
  const bannerColor = resolveColorHex(getSeverityPreset("danger").pill.fg);

  const isLoading = state.phase === "loading";
  const view = state.phase === "resolved" ? state.view : null;
  const typeConfirm = view?.runAffordance === "type-confirm";

  // Local UI state: the type-confirm field value and the [Cancel] [Run] focus.
  const [confirmValue, setConfirmValue] = useState("");
  const [focusedIndex, setFocusedIndex] = useState(0); // defaults to Cancel

  // Esc / Ctrl+C cancel in every phase.
  useKeyBindings([
    { on: "escape", do: onCancel },
    { on: { key: "c", ctrl: true }, do: onCancel },
  ]);

  // Button-affordance nav (clear / caution / no-llm / analysis-failed). The
  // type-confirm input owns keystrokes itself, and loading has only Cancel, so
  // arrow nav + Enter-on-focus is live only for the two-button case.
  const buttonNavActive = !isLoading && !typeConfirm;
  useKeyBindings(
    [
      { on: "left", do: () => setFocusedIndex(0) },
      { on: "right", do: () => setFocusedIndex(1) },
      { on: "return", do: () => (focusedIndex === 1 ? onRun() : onCancel()) },
    ],
    { isActive: buttonNavActive },
  );

  const spinnerActive = isLoading;
  const { frame: spinnerIndex } = useAnimation({ interval: SPINNER_INTERVAL, isActive: spinnerActive });
  const spinnerFrame = SPINNER_FRAMES[spinnerIndex % SPINNER_FRAMES.length] ?? "";

  const source = state.phase === "loading" ? state.source : state.view.source;

  const handleConfirmSubmit = (text: string) => {
    if (text.trim() === CONFIRM_WORD) {
      onRun();
    } else {
      setConfirmValue("");
    }
  };

  return (
    <Dialog gradientStops={frameStops(state, neutralGradient)} naturalContentWidth={NATURAL_CONTENT_WIDTH}>
      {/* Header line: severity pill / banner / bare source. */}
      {view?.state === "caution" || view?.state === "danger" ? (
        <Box>
          <Pill {...getSeverityPreset(view.state).pill} nerdFonts={nerdFonts} />
          <Text color={bodyColor}>{`  ${source}`}</Text>
        </Box>
      ) : view?.state === "manipulation" ? (
        <Box flexDirection="column">
          <Text color={bannerColor} bold>{`⚠ ${view.banner}`}</Text>
          <Text color={bodyColor}>{source}</Text>
        </Box>
      ) : (
        <Text color={bodyColor}>{source}</Text>
      )}

      {/* Body. */}
      {isLoading ? (
        <Box marginTop={1}>
          <Text color={bodyColor}>{`${spinnerFrame} Analyzing…`}</Text>
        </Box>
      ) : view && (view.state === "no-llm" || view.state === "analysis-failed") ? (
        <Box marginTop={1}>
          <Text color={bodyColor}>{view.message}</Text>
        </Box>
      ) : view ? (
        <>
          <Box marginTop={1}>
            <Text color={bodyColor}>{view.verdict}</Text>
          </Box>
          <FlagList flags={view.flags} color={supportingColor} />
          <BehaviorList behaviors={view.behaviors} color={supportingColor} />
        </>
      ) : null}

      {/* Action area. */}
      <Box marginTop={1}>
        {isLoading ? (
          <ActionBar items={[{ glyph: "Esc", label: "Cancel" }]} />
        ) : typeConfirm ? (
          <Box flexDirection="column">
            <ActionBar items={[{ glyph: "Esc", label: "Cancel" }]} />
            <Box marginTop={1}>
              <Text color={supportingColor}>{CONFIRM_PROMPT}</Text>
            </Box>
            <TextInput
              value={confirmValue}
              onChange={setConfirmValue}
              onSubmit={handleConfirmSubmit}
            />
          </Box>
        ) : (
          <ActionBar items={BUTTON_ACTIONS} focusedIndex={focusedIndex} dividerAfter={[0]} />
        )}
      </Box>
    </Dialog>
  );
}

const BUTTON_ACTIONS: ActionItem[] = [
  { glyph: "Esc", label: "Cancel" },
  { glyph: "Enter", label: "Run", primary: true },
];
