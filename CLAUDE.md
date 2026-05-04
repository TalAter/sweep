# Sweep

## Stack

- **Runtime:** Bun (TypeScript). Use `bun add` / `bun add -D` for dependencies. Never npm or pnpm.
- **Lint/format:** Biome + tsc (`bun run lint` = biome --write + typecheck).
- **Test:** `bun test` (files in `tests/`).
- **Full check:** `bun run check` = lint + test.

## wrap-core dependency

wrap-core is a sibling package providing shared substrate. When working on shared substrate (TUI primitives, theme, providers, dialog infra, config), read `vault/wrap-core-api`.
