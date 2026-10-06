use std::process::{Command, Output};
use tempfile::TempDir;

fn sweep(home: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sweep"))
        .args(args)
        .env("SWEEP_HOME", home.path())
        .env_remove("SWEEP_CONFIG")
        .env_remove("SWEEP_TEST_RESPONSES")
        .output()
        .unwrap()
}

#[test]
fn empty_list_is_payload_only_and_creates_compatible_home() {
    let home = TempDir::new().unwrap();
    let output = sweep(&home, &["list"]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "No packages installed.\n"
    );
    assert!(output.stderr.is_empty());
    assert!(home.path().is_dir());
    assert!(!home.path().join("config.jsonc").exists());
    let db = rusqlite::Connection::open(home.path().join("sweep.db")).unwrap();
    let n: i64 = db
        .query_row("SELECT COUNT(*) FROM invocations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn parse_failure_records_one_trimmed_attempt_and_exits_two() {
    let home = TempDir::new().unwrap();
    let output = sweep(&home, &["  nonsense  "]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .starts_with("sweep: unrecognized install command shape:")
    );
    let db = rusqlite::Connection::open(home.path().join("sweep.db")).unwrap();
    let rows: (i64, String, String, Option<i64>) = db
        .query_row(
            "SELECT COUNT(*), raw_input, outcome, package_id FROM invocations",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(rows, (1, "nonsense".into(), "parse_failed".into(), None));
}

#[test]
fn absent_command_without_terminal_is_parse_failure() {
    let home = TempDir::new().unwrap();
    let output = sweep(&home, &[]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "sweep: empty input\n"
    );
}

#[test]
fn malformed_config_fails_before_dispatch_and_env_overlay_keeps_unknown_fields() {
    let home = TempDir::new().unwrap();
    std::fs::write(home.path().join("config.jsonc"), "{ broken").unwrap();
    let output = sweep(&home, &["list"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("config.jsonc"));
    assert!(output.stdout.is_empty());
    std::fs::write(
        home.path().join("config.jsonc"),
        "{ // comments\n\"future\": true, }",
    )
    .unwrap();
    assert!(sweep(&home, &["list"]).status.success());
}

#[test]
fn existing_database_is_read_without_rewriting_installed_or_failed_packages() {
    let home = TempDir::new().unwrap();
    let db = rusqlite::Connection::open(home.path().join("sweep.db")).unwrap();
    db.execute_batch(include_str!("fixtures/schema-v1.sql"))
        .unwrap();
    let output = sweep(&home, &["list", "ignored-argument"]);
    assert!(output.status.success());
    let out = String::from_utf8_lossy(&output.stdout);
    for text in [
        "PACKAGE",
        "SOURCE",
        "STATUS",
        "LAST RAN",
        "old-tool",
        "example.org",
        "installed",
        "2025-01-03",
    ] {
        assert!(out.contains(text), "missing {text}: {out}");
    }
    assert!(!out.contains("failed-tool"));
    assert!(!out.contains('\u{1b}'));
    assert!(output.stderr.is_empty());
    let values: (String,String,i64) = db.query_row("SELECT current_sha256, installed_at, (SELECT COUNT(*) FROM invocations) FROM packages WHERE id=7", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(
        values,
        ("abc123".into(), "2025-01-01T00:00:00.000Z".into(), 1)
    );
}

#[test]
fn unavailable_terminal_never_runs_and_records_committed_command() {
    let home = TempDir::new().unwrap();
    let output = sweep(&home, &["curl http://127.0.0.1:1/script | sh"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let db = rusqlite::Connection::open(home.path().join("sweep.db")).unwrap();
    let row: (i64, String, Option<i32>) = db
        .query_row(
            "SELECT COUNT(*),outcome,exit_code FROM invocations",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(row, (1, "errored".into(), None));
}
