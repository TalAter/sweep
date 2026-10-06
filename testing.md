# Testing

Every test must protect a behavioral contract and catch a realistic bug. Audit
existing coverage before converting a behavior; add missing contract tests to
the reference implementation when useful, then carry those contracts into Rust.

- Write the smallest meaningful failing test before implementation. Run it and
  confirm an assertion fails for the intended behavior, not a missing symbol,
  compile error, or broken fixture. Implement, get green, then refactor. Use
  compilable stubs when needed to establish a genuine failing assertion.
- Prioritize parsing, approval gates, lifecycle transitions, exactly one
  invocation per install attempt, configuration compatibility, persisted data,
  process execution, and consequential failure paths. No coverage quotas.
- Assert observable behavior through the most public practical boundary. Use
  integration tests in `tests/` for CLI and persistence contracts; module tests
  are appropriate for isolated logic. Derive expectations from contracts and
  reference observations, not by copying implementation.
- Skip exports, compiler-enforced types, trivial accessors, private call order,
  literal constants, and broad snapshots. Focused Ratatui buffer assertions are
  useful for meaningful layout, styling, selection, and overflow contracts.
- Exercise real code. Substitute only external or nondeterministic boundaries,
  such as HTTP services, provider responses, clocks, and terminal events. Do
  not mock away the behavior being tested.
- Isolate filesystem and SQLite tests with disposable temporary directories.
  Pass `SWEEP_HOME` to child processes; avoid process-global environment changes
  in parallel tests. Verify migrations against legacy database fixtures and
  configuration fixtures, including unknown fields and failure cases.
- Installer fixtures must be harmless even if accidentally executed: use
  `echo`, `true`, `false`, or `exit 3`. Never use destructive commands, privilege
  escalation, or real network installers. Use local HTTP fixtures for downloads.
- Test CLI exit codes, stdout/stderr separation, persisted outcomes, approval
  refusal, and subprocess side effects. Assert that unapproved scripts do not run.
- Test UI state transitions and rendering deterministically, then exercise a
  real terminal or PTY: keyboard navigation, resize, scrolling, dialog approval
  and cancellation, subprocess handoff, and terminal restoration on failure.
  A passing unit suite alone does not verify terminal behavior or visual fidelity.
- Use disposable Linux containers for platform integration and installer flows;
  run native macOS CLI and terminal checks too. Neither substitutes for the other.
- Fix regressions rather than weakening assertions. Investigate reference bugs
  before preserving them. Remove redundant tests when their contracts remain
  covered; document intentional behavior corrections.

Run focused `cargo test <filter>` or `cargo test --test <target>` while working.
Before committing, run formatting and Clippy checks from AGENTS.md and relevant
broader tests. Before completing the conversion, run the full Rust suite and
integrated platform/terminal checks. Keep every commit green and atomic; report
what ran and any gaps. Documentation-only changes need direct verification,
not ceremonial tests.
