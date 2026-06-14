import { resolveAppearance, resolveTheme, setTheme } from "wrap-core/theme";
import { openDialog, preloadDialogRuntime } from "wrap-core/tui";
import { sweepFs } from "../fs.ts";
import { DARK_GRADIENT, LIGHT_GRADIENT } from "./theme.ts";

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
