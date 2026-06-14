import type { Color } from "wrap-core/ansi";
import { getTheme } from "wrap-core/theme";
import type { PillSegment } from "wrap-core/tui";

// ── Base gradient ─────────────────────────────────────────────────
//
// Sweep's own neutral install-dialog frame for the no-pill states (clear /
// no-LLM / analysis-failed / paste) — per product spec, absence of red flags
// is not an endorsement, so those frames stay neutral.
//
// Kept app-local rather than promoted to core: the dark variant is
// sweep-specific (it deliberately doesn't couple to wrap's wizard frame or to
// unrelated core copy/checklist tokens). The light variant happens to coincide
// with those neutral tokens, but stays here as one unit with dark. (Contrast
// the severity palette, which was byte-identical across apps and so was
// promoted to core; the dark frame differs, so this one isn't.)

export const DARK_GRADIENT: Color[] = [
  [80, 160, 255],
  [40, 60, 100],
];
export const LIGHT_GRADIENT: Color[] = [
  [25, 90, 190],
  [170, 170, 195],
];

// ── Severity presets ──────────────────────────────────────────────

type SeverityPreset = { stops: Color[]; pill: PillSegment };

/** Call at render time — setTheme must have run.
 *  There is intentionally no `clear` preset: clear has no pill.
 *
 *  Sweep's `caution` maps to core's `warning` — a deliberate vocab mapping
 *  (sweep speaks caution/danger; core speaks warning/danger). */
export function getSeverityPreset(level: "caution" | "danger"): SeverityPreset {
  const severity = getTheme().severity;
  switch (level) {
    case "caution":
      return {
        stops: severity.warning.frame,
        pill: { ...severity.warning.pill, label: "⚠ caution", bold: true },
      };
    case "danger":
      return {
        stops: severity.danger.frame,
        pill: { ...severity.danger.pill, label: "✗ danger", bold: true },
      };
  }
}
