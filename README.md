# Sweep

**Brew for what you cannot brew.**

Sweep is a package manager for the long tail of dev tools that ship via `curl https://… | sh`. It gives those installs `list` / `update` / `away` (uninstall) ergonomics without the publisher shipping a formula, and reads the install script before you run it — surfacing what's about to happen and any red flags.

Design docs live in [vault/](vault/README.md), starting with the [product spec](vault/product-spec.md).

## Development

```sh
bun run start       # run from source
bun test            # test suite
bun run lint        # biome --write + tsc
bun run check       # lint + tests
bun run build       # compile binaries to dist/
bun run sandbox     # Docker sandbox for end-to-end installer runs
```
