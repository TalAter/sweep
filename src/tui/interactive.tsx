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

type InteractiveDialogProps = {
  gradientStops: Color[];
  onSubmit: (command: string) => void;
  onCancel: () => void;
};

export function InteractiveDialog({ gradientStops, onSubmit, onCancel }: InteractiveDialogProps) {
  const [value, setValue] = useState("");
  const theme = useTheme();
  const promptColor = resolveColorHex(theme.dialog.prompt);

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
    <Dialog gradientStops={gradientStops} naturalContentWidth={50}>
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
          <Box marginTop={1}>
            <ActionBar items={actions} />
          </Box>
        </>
      )}
    </Dialog>
  );
}
