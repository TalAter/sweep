# Sweep

Sweep inspects `curl … | sh` installers before you run them, then keeps a local
record of installs. Written in Rust with a custom Ratatui terminal UI.

```sh
sweep 'curl -fsSL https://example.com/install.sh | sh'
sweep       # paste an install command interactively
sweep list  # installed packages
```

The review shows what the script does and concerns worth examining. Analysis is
optional: missing or failed analysis is shown explicitly. Execution always needs
approval. Cancellation never executes the script. Updating and uninstalling
packages are not implemented.

## Configuration and data

Sweep stores `config.jsonc`, `sweep.db`, and `cache/scripts/<sha256>` under
`~/.sweep/`, or `$SWEEP_HOME`. Back up this directory as a unit.
Sweep reads configuration; it does not create a provider configuration for you.

Example `config.jsonc`:

```jsonc
{
  "defaultProvider": "openai",
  "providers": {
    "openai": { "model": "your-model", "apiKey": "$OPENAI_API_KEY" }
  }
}
```

Providers: Anthropic, OpenAI, OpenRouter, Groq, Mistral, Ollama, Claude Code, and
custom OpenAI-compatible endpoints. Groq, Mistral, and Ollama require `baseURL`;
custom endpoints require `baseURL`, `apiKey`, and `model`. Claude Code uses the
installed `claude` CLI and its authentication; its `model` is optional.
`SWEEP_CONFIG` supplies a strict JSON overlay, replacing top-level fields rather
than merging nested objects. `SWEEP_THEME=light` or `dark` overrides appearance.

## Development

Install a current stable Rust toolchain. Read [AGENTS.md](AGENTS.md) and
[testing.md](testing.md).

```sh
cargo run -- list
cargo run -- 'curl https://example.com/install.sh | sh'
cargo build --release          # target/release/sweep
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
make check                    # formatting, Clippy, tests
make terminal                 # real PTY interaction checks
make linux-test               # disposable Linux container checks
```

Develop and verify the UI natively on macOS; use Docker-compatible containers
(including OrbStack) for Linux checks and disposable installer experiments.
`scripts/sandbox.sh` supports `up`, `down`, `kill`, `rebuild`, and `test`.

See [architecture](docs/architecture.md) and [glossary](GLOSSARY.md) for the
few contracts that are not obvious from module names.
