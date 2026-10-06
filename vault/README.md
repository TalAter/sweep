# Sweep glossary

Sweep reviews shell installers, runs approved scripts, and lists successfully
installed packages. The implementation defines scope; this vault is not a roadmap.
See [architecture](architecture.md) for flow and invariants.

- **Sweep home** — `~/.sweep/`, overridden by `$SWEEP_HOME`; contains configuration,
  SQLite history, and cached scripts.
- **Install command** — parsed user input: URL, runner shell, environment values,
  optional sudo, script arguments, and the original command text.
- **Fetcher** — `curl` or `wget` in that input. Sweep downloads the URL itself;
  fetcher flags do not carry over.
- **Runner shell** — `sh`, `bash`, or `zsh`; receives the approved script on stdin.
- **Invocation** — one committed install attempt, including failures and declines.
  Cancelling before committing an interactive command creates no invocation.
- **Package** — a row keyed by the original source URL, created on the approved
  execution path. Its slug is a display name derived from the URL hostname.
- **Install** — execution that returned zero. A later failed execution does not
  erase an earlier successful installation.
- **Script store / CAS** — content-addressed raw bytes at
  `cache/scripts/<sha256>`. A declined invocation may record a hash without
  creating a cached script.
- **Analysis pass** — describes behavior, concerns, severity, and summary.
- **Manipulation pass** — independently checks attempts to steer the analyzer.
  Only an explicitly clean result permits displaying the summary as trusted.
