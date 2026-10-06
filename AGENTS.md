# Sweep

A Rust CLI for inspecting and managing tools installed by shell scripts, with a
Ratatui terminal UI. Keep project-specific code here; no sibling-repository
runtime or build dependencies. Prefer established, maintained Rust libraries.

Read [testing.md](testing.md) before writing tests or implementation. Work
test-first and make **green atomic commits**: each commit has one coherent
purpose, passes relevant checks, and explains why the change is needed.

Preserve CLI behavior, terminal quality, configuration compatibility, and
persisted data. During conversion, use the existing implementation as evidence;
investigate discrepancies. Future features in old notes are not conversion scope.
Keep documentation minimal: high-level architecture, non-obvious constraints,
and the glossary. Read relevant local architecture notes before changing a domain.

Use Cargo for Rust dependencies and workflows. Run focused tests while working;
before committing Rust changes run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and relevant `cargo test` coverage. Verify integrated CLI and
terminal flows as described in testing.md. Do not rely on a stop hook.

`~/.sweep/config.jsonc` (or `$SWEEP_HOME/config.jsonc`) contains live provider
keys. Never print it without redacting credentials. Tests use disposable Sweep
homes, never the live home. Keep responses and commit messages concise.
