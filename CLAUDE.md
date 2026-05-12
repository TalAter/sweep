# Sweep

## Stack

- **Runtime:** Bun (TypeScript). Use `bun add` / `bun add -D` for dependencies. Never npm or pnpm.
- **Lint/format:** Biome + tsc (`bun run lint` = biome --write + typecheck).
- **Test:** `bun test` (files in `tests/`).
- **Full check:** `bun run check` = lint + test.

## wrap-core dependency

Sweep is a sibling of [Wrap](../wrap/), sharing substrate via the `wrap-core` package. Different domain, similar bones.
When working on shared substrate (TUI primitives, theme, providers, dialog infra, config), read `vault/wrap-core-api`.

When scaffolding something sweep doesn't yet have an opinion on (tests, vault, config, prompt shape, CI, release), mirror how wrap does it unless there's a clear reason to diverge — wrap is the mature sibling and our conventions live there. Sweep's own conventions emerge as domain pressure shows up.