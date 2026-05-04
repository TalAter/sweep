# Sweep

## Stack

- **Runtime:** Bun (TypeScript). Use `bun add` / `bun add -D` for dependencies. Never npm or pnpm.
- **Lint/format:** Biome + tsc (`bun run lint` = biome --write + typecheck).
- **Test:** `bun test` (files in `tests/`).
- **Full check:** `bun run check` = lint + test.
