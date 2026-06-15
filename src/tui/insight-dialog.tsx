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
import { stringWidth } from "wrap-core/text";
import {
  ActionBar,
  type ActionItem,
  actionBarWidth,
  Dialog,
  Pill,
  pillWidth,
  type SizeBasis,
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

// Fixed chrome the renderer adds around the model's text — defined once and shared
// by the JSX and by `insightContentWidth` so the measured width can't drift from
// what's actually drawn.
const FLAGS_LABEL = "Flags:";
const FLAG_PREFIX = " ⚠ ";
const BEHAVIORS_LABEL = "Appears to do (not exhaustive):";
const BEHAVIOR_PREFIX = " • ";
const SUDO_SUFFIX = "  (sudo)";
const BANNER_PREFIX = "⚠ ";
const PILL_GAP = "  "; // between the severity pill and the source on the header line
const ANALYZING_LABEL = "Analyzing…";

// Action sets — shared by the JSX and by `insightSizeTo`'s measurement so the bar
// can never be wider than the dialog it sits in.
const CANCEL_ONLY_ACTIONS: ActionItem[] = [{ glyph: "Esc", label: "Cancel" }];
const BUTTON_ACTIONS: ActionItem[] = [
  { glyph: "Esc", label: "Cancel" },
  { glyph: "Enter", label: "Run", primary: true },
];
const BUTTON_DIVIDER_AFTER = [0]; // a divider after Cancel only

// Aesthetic floor on the dialog width — the smallest it may get before the
// terminal clamp. Not an action-bar proxy: the bar is measured below; this just
// stops a terse loading/summary box from looking cramped.
const MIN_CONTENT_WIDTH = 44;

/**
 * The lines and pre-measured widths the current state will render — fed to
 * `Dialog` as `sizeTo`. Plain text goes in as strings (Dialog measures them); the
 * pill header, the spinner line, and the action bar go in as numbers (already
 * measured, since they're not plain text). Dialog takes the max, floors at
 * `MIN_CONTENT_WIDTH`, and clamps to the terminal — so the box is compact while
 * analyzing and grows with the analysis prose, capping at screen width.
 */
function insightSizeTo(state: InsightDialogState, nerd: boolean): SizeBasis[] {
  if (state.phase === "loading") {
    return [
      state.source,
      2 + stringWidth(ANALYZING_LABEL), // spinner glyph + space + label
      actionBarWidth(CANCEL_ONLY_ACTIONS),
    ];
  }

  const { view } = state;
  const basis: SizeBasis[] = [];

  // Header line.
  if (view.state === "caution" || view.state === "danger") {
    const pill = getSeverityPreset(view.state).pill;
    basis.push(pillWidth([pill], nerd, false) + stringWidth(PILL_GAP + view.source));
  } else if (view.state === "manipulation") {
    basis.push(BANNER_PREFIX + view.banner, view.source);
  } else {
    basis.push(view.source);
  }

  // Body.
  if (view.state === "no-llm" || view.state === "analysis-failed") {
    basis.push(view.message);
  } else {
    basis.push(view.summary);
    if (view.flags.length > 0) {
      basis.push(FLAGS_LABEL, ...view.flags.map((flag) => FLAG_PREFIX + flag));
    }
    if (view.behaviors.length > 0) {
      basis.push(
        BEHAVIORS_LABEL,
        ...view.behaviors.map((b) => BEHAVIOR_PREFIX + b.description + (b.sudo ? SUDO_SUFFIX : "")),
      );
    }
  }

  // Action area.
  if (view.runAffordance === "type-confirm") {
    basis.push(actionBarWidth(CANCEL_ONLY_ACTIONS), CONFIRM_PROMPT);
  } else {
    basis.push(actionBarWidth(BUTTON_ACTIONS, BUTTON_DIVIDER_AFTER));
  }

  return basis;
}

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
      <Text color={color}>{BEHAVIORS_LABEL}</Text>
      {behaviors.map((b) => (
        <Text key={b.description} color={color}>
          {BEHAVIOR_PREFIX}
          {b.description}
          {b.sudo ? SUDO_SUFFIX : ""}
        </Text>
      ))}
    </Box>
  );
}

function FlagList({ flags, color }: { flags: string[]; color: string }) {
  if (flags.length === 0) return null;
  return (
    <Box flexDirection="column" marginTop={1}>
      <Text color={color}>{FLAGS_LABEL}</Text>
      {flags.map((flag) => (
        <Text key={flag} color={color}>
          {FLAG_PREFIX}
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
  const { frame: spinnerIndex } = useAnimation({
    interval: SPINNER_INTERVAL,
    isActive: spinnerActive,
  });
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
    <Dialog
      gradientStops={frameStops(state, neutralGradient)}
      sizeTo={insightSizeTo(state, nerdFonts)}
      minContentWidth={MIN_CONTENT_WIDTH}
    >
      {/* Header line: severity pill / banner / bare source. */}
      {view?.state === "caution" || view?.state === "danger" ? (
        <Box>
          <Pill {...getSeverityPreset(view.state).pill} nerdFonts={nerdFonts} />
          <Text color={bodyColor}>{`${PILL_GAP}${source}`}</Text>
        </Box>
      ) : view?.state === "manipulation" ? (
        <Box flexDirection="column">
          <Text color={bannerColor} bold>{`${BANNER_PREFIX}${view.banner}`}</Text>
          <Text color={bodyColor}>{source}</Text>
        </Box>
      ) : (
        <Text color={bodyColor}>{source}</Text>
      )}

      {/* Body. */}
      {isLoading ? (
        <Box marginTop={1}>
          <Text color={bodyColor}>{`${spinnerFrame} ${ANALYZING_LABEL}`}</Text>
        </Box>
      ) : view && (view.state === "no-llm" || view.state === "analysis-failed") ? (
        <Box marginTop={1}>
          <Text color={bodyColor}>{view.message}</Text>
        </Box>
      ) : view ? (
        <>
          <Box marginTop={1}>
            <Text color={bodyColor}>{view.summary}</Text>
          </Box>
          <FlagList flags={view.flags} color={supportingColor} />
          <BehaviorList behaviors={view.behaviors} color={supportingColor} />
        </>
      ) : null}

      {/* Action area. */}
      <Box marginTop={1}>
        {isLoading ? (
          <ActionBar items={CANCEL_ONLY_ACTIONS} />
        ) : typeConfirm ? (
          <Box flexDirection="column">
            <ActionBar items={CANCEL_ONLY_ACTIONS} />
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
          <ActionBar
            items={BUTTON_ACTIONS}
            focusedIndex={focusedIndex}
            dividerAfter={BUTTON_DIVIDER_AFTER}
          />
        )}
      </Box>
    </Dialog>
  );
}
