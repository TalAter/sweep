import type { Color } from "wrap-core/ansi";
import { resolveAppearance, resolveTheme, setTheme } from "wrap-core/theme";
import { chooseDialogStdin, DIALOG_INK_OPTIONS } from "wrap-core/tui";
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

  const [ink, react, { InteractiveDialog }, { ThemeProvider }] = await Promise.all([
    import("ink"),
    import("react"),
    import("./interactive.tsx"),
    import("wrap-core/tui"),
  ]);

  return new Promise<string | null>((resolve) => {
    const { stream: stdin, fd: ownedFd } = chooseDialogStdin();

    const cleanup = () => {
      app.unmount();
      if (ownedFd !== null && typeof (stdin as { destroy?: () => void }).destroy === "function") {
        (stdin as { destroy: () => void }).destroy();
      }
    };

    const onSubmit = (command: string) => {
      cleanup();
      resolve(command);
    };
    const onCancel = () => {
      cleanup();
      resolve(null);
    };

    const app = ink.render(
      react.createElement(ThemeProvider, {
        theme,
        nerdFonts: false,
        children: react.createElement(InteractiveDialog, { gradientStops, onSubmit, onCancel }),
      }),
      { ...DIALOG_INK_OPTIONS, stdin },
    );
  });
}
