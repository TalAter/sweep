import { Box, Text } from "ink";
import { useState } from "react";
import type { Color } from "wrap-core/ansi";
import { resolveColorHex } from "wrap-core/ansi";
import {
  ActionBar,
  type ActionItem,
  Dialog,
  TextInput,
  useKeyBindings,
  useTheme,
} from "wrap-core/tui";
import { getSeverityPreset } from "./theme.ts";

type InteractiveDialogProps = {
  gradientStops: Color[];
  onSubmit: (command: string) => void;
  onCancel: () => void;
  /**
   * Inline parse error. Set by the step-5 session controller when a pasted
   * command fails `parseInstallCommand`: the session stays on this paste input
   * and surfaces the message HERE (not as a `sweep:` stderr line, which the live
   * alt-screen would hide). Undefined in the normal first-paste case.
   */
  error?: string;
};

export function InteractiveDialog({
  gradientStops,
  onSubmit,
  onCancel,
  error,
}: InteractiveDialogProps) {
  const [value, setValue] = useState("");
  const theme = useTheme();
  const promptColor = resolveColorHex(theme.dialog.prompt);
  // Borrow the danger pill's fg for the error line — the loudest token we have,
  // routed through getSeverityPreset like InsightDialog's banner rather than
  // reaching into theme.severity directly.
  const errorColor = resolveColorHex(getSeverityPreset("danger").pill.fg);

  useKeyBindings([
    { on: "escape", do: onCancel },
    { on: { key: "c", ctrl: true }, do: onCancel },
  ]);

  const actions: ActionItem[] = [
    { glyph: "Enter", label: "Install", primary: true },
    { glyph: "Esc", label: "Cancel" },
  ];

  const handleSubmit = (text: string) => {
    const trimmed = text.trim();
    if (trimmed) onSubmit(trimmed);
  };

  return (
    <Dialog gradientStops={gradientStops} sizeTo={[50]}>
      {(_innerWidth: number) => (
        <>
          <Box marginBottom={1}>
            <Text color={promptColor}>Paste an install command</Text>
          </Box>
          <TextInput
            value={value}
            onChange={setValue}
            onSubmit={handleSubmit}
            placeholder="curl ... | sh"
          />
          {error ? (
            <Box marginTop={1}>
              <Text color={errorColor}>{error}</Text>
            </Box>
          ) : null}
          <Box marginTop={1}>
            <ActionBar items={actions} />
          </Box>
        </>
      )}
    </Dialog>
  );
}
