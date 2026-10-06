# Testing

Tests protect observable behavior and catch realistic bugs. Audit coverage before
changing a behavior; add a meaningful regression test for missing contracts.

- Write a failing test before implementation. Confirm an assertion fails for the
  intended behavior, not a missing symbol or compile error. Implement, get green,
  then refactor. Each commit must be green and atomic.
- Prioritize approval gates, parser boundaries, lifecycle transitions, persisted
  outcomes, configuration, subprocess side effects, and failure paths. Skip
  exports, type assertions, trivial getters, and other compiler-checked plumbing.
- Exercise real code through public boundaries. Substitute external services,
  clocks, and terminal events when needed; don't mock away the behavior tested.
- Use temporary homes and local HTTP fixtures. Pass `SWEEP_HOME` to subprocesses;
  don't mutate process-global environment in parallel tests. Verify existing
  database and configuration fixtures remain readable, including unknown fields.
- Execution fixtures must be harmless: `echo`, `true`, `false`, or `exit 3`.
  Real-world command examples belong in parser-only fixtures and are never run.
- Check exit codes, stdout/stderr separation, persisted results, and the absence
  of side effects before approval. Test failures as well as successful installs.
- Test UI state and focused Ratatui buffer contracts, then exercise a real PTY:
  navigation, Unicode editing, resize, scrolling, approval, cancellation, child
  terminal handoff, and restoration after errors and signals.
- Use native macOS and disposable Linux container checks; neither replaces the
  other. Fix regressions instead of weakening assertions. Remove redundant tests
  when their contracts remain covered.

Use focused `cargo test <filter>` or `cargo test --test <target>` while working.
Before committing Rust changes, run `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, and relevant tests. For integrated
changes, run `make check`, `make terminal`, and `make linux-test`. Report gaps
explicitly. Documentation-only edits need direct verification, not new tests.
