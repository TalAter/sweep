# Sweep

## Stack

- **Runtime:** Bun (TypeScript). Use `bun add` / `bun add -D` for dependencies. Never npm or pnpm.
- **Lint/format:** Biome + tsc (`bun run lint` = biome --write + typecheck).
- **Test:** `bun test` (files in `tests/`). Run specific tests with `bun test tests/foo.test.ts`.
- **Full check:** `bun run check` = lint + test.

## Testing — TDD

All implementation follows TDD. Always write a failing test before writing code. No exceptions. Aim for maximum test coverage, but tests must earn their place — skip tests that only prove plumbing (exports, imports, type-only assertions). **Before writing tests or implementation, read `.claude/skills/testing.md`.**

## Vault

Sweep has a vault at `vault/`. The index is auto-loaded below. When working on a concept, read `vault/<concept>.md` before diving into code.

@vault/README.md

## Stop hook

A stop hook runs `bun run lint` (biome --write + tsc) automatically when you finish. Don't run lint/format/tsc as a final check before stopping — they'll just run twice. Tests are **not** in the stop hook — run them yourself when needed, preferring targeted runs (`bun test tests/foo.test.ts`) over the full suite.

## wrap-core dependency

Sweep is a sibling of [Wrap](../wrap/), sharing substrate via the `wrap-core` package. Different domain, similar bones.
When working on shared substrate (TUI primitives, theme, providers, dialog infra, config), read `vault/wrap-core-api/` — a directory; start at its README.

wrap-core is unpublished; it resolves via local workspace linking and a separate CI-only mechanism that can silently drift out of sync. Before touching dependency wiring, build scripts, or release workflows, read `vault/dependency-model.md`.

When scaffolding something sweep doesn't yet have an opinion on (tests, vault, config, prompt shape, CI, release), mirror how wrap does it unless there's a clear reason to diverge — wrap is the mature sibling and our conventions live there. Sweep's own conventions emerge as domain pressure shows up.

## Secrets

`~/.sweep/config.jsonc` (and `$SWEEP_HOME/config.jsonc`) holds live provider API keys. Never `cat` it raw. Read it with the `apiKey` values stripped:

```sh
sed -E 's/("apiKey"[[:space:]]*:[[:space:]]*")[^"]*"/\1<redacted>"/' ~/.sweep/config.jsonc
```
