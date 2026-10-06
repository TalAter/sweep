# Sweep

Rust CLI with a custom Ratatui terminal UI. Prefer established, maintained crates;
keep project-specific behavior in this repository.

Read [testing.md](testing.md) before writing tests or implementation. Work
test-first and make **green atomic commits**: one coherent purpose, relevant
checks passing, and a concise commit message explaining why.

Preserve CLI behavior, terminal quality, configuration, and persisted data.
Read [architecture](docs/architecture.md) for non-obvious contracts and
[GLOSSARY.md](GLOSSARY.md) for domain vocabulary. Keep docs concise and current.

Use Cargo for dependencies and workflows. Before committing Rust changes, run
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and relevant
`cargo test` coverage. Verify terminal interactions when UI behavior changes.

`~/.sweep/config.jsonc` (or `$SWEEP_HOME/config.jsonc`) contains live provider
keys. Never print credentials. Tests use disposable homes, never the live home.
Keep responses concise.
