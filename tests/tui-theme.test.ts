import { afterEach, describe, expect, test } from "bun:test";
import { DARK_CORE, LIGHT_CORE, resolveTheme, setTheme } from "wrap-core/theme";
import { getSeverityPreset } from "../src/tui/theme.ts";

afterEach(() => {
  // Reset to a known theme so other test files aren't affected.
  setTheme(resolveTheme("dark"));
});

describe("getSeverityPreset", () => {
  test("danger reads label + pill + stops from the active core theme (dark)", () => {
    setTheme(resolveTheme("dark"));
    const preset = getSeverityPreset("danger");
    expect(preset.pill.label).toBe("✗ danger");
    expect(preset.pill.bold).toBe(true);
    expect(preset.pill.fg).toEqual(DARK_CORE.severity.danger.pill.fg);
    expect(preset.pill.bg).toEqual(DARK_CORE.severity.danger.pill.bg);
    expect(preset.stops).toEqual(DARK_CORE.severity.danger.frame);
  });

  test("caution maps to core.warning, reading label + pill + stops (dark)", () => {
    setTheme(resolveTheme("dark"));
    const preset = getSeverityPreset("caution");
    expect(preset.pill.label).toBe("⚠ caution");
    expect(preset.pill.bold).toBe(true);
    expect(preset.pill.fg).toEqual(DARK_CORE.severity.warning.pill.fg);
    expect(preset.pill.bg).toEqual(DARK_CORE.severity.warning.pill.bg);
    expect(preset.stops).toEqual(DARK_CORE.severity.warning.frame);
  });

  test("reads the active theme — light values differ from dark", () => {
    setTheme(resolveTheme("light"));
    const preset = getSeverityPreset("danger");
    expect(preset.pill.fg).toEqual(LIGHT_CORE.severity.danger.pill.fg);
    expect(preset.stops).toEqual(LIGHT_CORE.severity.danger.frame);
    // Guard the "active theme" contract: light differs from dark.
    expect(preset.pill.fg).not.toEqual(DARK_CORE.severity.danger.pill.fg);
  });
});
