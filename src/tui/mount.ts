import type { Color } from "wrap-core/ansi";
import { resolveAppearance, resolveTheme, setTheme } from "wrap-core/theme";
import { openDialog, preloadDialogRuntime } from "wrap-core/tui";
import { sweepFs } from "../fs.ts";

const DARK_GRADIENT: Color[] = [
  [80, 160, 255],
  [40, 60, 100],
];
const LIGHT_GRADIENT: Color[] = [
  [25, 90, 190],
  [170, 170, 195],
];

export async function promptInstallCommand(): Promise<string | null> {
  const appearance = await resolveAppearance({ envVarName: "SWEEP_THEME", fs: sweepFs });
  const theme = resolveTheme(appearance);
  setTheme(theme);

  const gradientStops = appearance === "light" ? LIGHT_GRADIENT : DARK_GRADIENT;

  const [react, { InteractiveDialog }] = await Promise.all([
    import("react"),
    import("./interactive.tsx"),
    preloadDialogRuntime(),
  ]);

  return openDialog<string | null>({ theme, nerdFonts: false }, (close) =>
    react.createElement(InteractiveDialog, {
      gradientStops,
      onSubmit: close,
      onCancel: () => close(null),
    }),
  );
}
