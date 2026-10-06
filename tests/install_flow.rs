use sha2::{Digest, Sha256};
use sweep::{
    app::finish_install,
    fetch::FetchedScript,
    parse::parse_install_command,
    store::{Store, now},
    tui::InstallDecision,
};
fn fetched(bytes: &[u8]) -> FetchedScript {
    FetchedScript {
        bytes: bytes.to_vec(),
        sha256: hex::encode(Sha256::digest(bytes)),
        final_url: "https://cdn.example.org/install".into(),
        fetched_at: now(),
        status: 200,
    }
}
fn decision(raw: &str, bytes: &[u8]) -> InstallDecision {
    InstallDecision::Run {
        raw: raw.into(),
        parsed: parse_install_command(raw).unwrap(),
        fetched: fetched(bytes),
    }
}
fn query(home: &std::path::Path, sql: &str) -> (i64, String, Option<i32>) {
    let db = rusqlite::Connection::open(home.join("sweep.db")).unwrap();
    db.query_row(sql, [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
}
#[test]
fn run_records_exact_bytes_and_preserves_install_on_failed_rerun() {
    let home = tempfile::tempdir().unwrap();
    let mut store = Store::open(home.path()).unwrap();
    let raw = "curl https://example.org/install | sh";
    assert_eq!(
        finish_install(&mut store, &now(), decision(raw, b"exit 0\n")).unwrap(),
        0
    );
    assert_eq!(
        finish_install(&mut store, &now(), decision(raw, b"exit 3\n")).unwrap(),
        3
    );
    assert_eq!(
        query(
            home.path(),
            "SELECT COUNT(*),max(outcome),max(exit_code) FROM invocations"
        ),
        (2, "ran".into(), Some(3))
    );
    let rows = store.list_installed_packages().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].current_sha256.as_deref(),
        Some(fetched(b"exit 0\n").sha256.as_str())
    );
    assert_eq!(
        store.read_script(&fetched(b"exit 3\n").sha256).unwrap(),
        Some(b"exit 3\n".to_vec())
    );
}
#[test]
fn cancel_before_commit_has_no_row_but_after_fetch_records_without_package_or_cache() {
    let home = tempfile::tempdir().unwrap();
    let mut store = Store::open(home.path()).unwrap();
    assert_eq!(
        finish_install(
            &mut store,
            &now(),
            InstallDecision::Cancel {
                raw: None,
                parsed: None,
                fetched: None
            }
        )
        .unwrap(),
        0
    );
    let raw = "curl https://example.org/install | sh";
    let f = fetched(b"exit 0\n");
    let sha = f.sha256.clone();
    assert_eq!(
        finish_install(
            &mut store,
            &now(),
            InstallDecision::Cancel {
                raw: Some(raw.into()),
                parsed: Some(parse_install_command(raw).unwrap()),
                fetched: Some(f)
            }
        )
        .unwrap(),
        130
    );
    assert_eq!(
        query(
            home.path(),
            "SELECT COUNT(*),outcome,package_id FROM invocations"
        ),
        (1, "cancelled".into(), None)
    );
    assert!(store.read_script(&sha).unwrap().is_none());
    assert!(store.list_installed_packages().unwrap().is_empty());
}
#[test]
fn spawn_failure_still_records_one_attempt() {
    let home = tempfile::tempdir().unwrap();
    let mut store = Store::open(home.path()).unwrap();
    let raw = "curl https://example.org/install | sh";
    let mut parsed = parse_install_command(raw).unwrap();
    parsed.shell = "/nonexistent/sweep-test-shell".into();
    let result = finish_install(
        &mut store,
        &now(),
        InstallDecision::Run {
            raw: raw.into(),
            parsed,
            fetched: fetched(b"exit 0\n"),
        },
    );
    assert!(result.is_err());
    assert_eq!(
        query(
            home.path(),
            "SELECT COUNT(*),outcome,exit_code FROM invocations"
        ),
        (1, "errored".into(), None)
    );
}

#[test]
fn listing_preserves_theme_roles_alignment_and_plain_pipe_output() {
    let home = tempfile::tempdir().unwrap();
    let mut store = Store::open(home.path()).unwrap();
    finish_install(
        &mut store,
        &now(),
        decision("curl https://www.example.org/install | sh", b"exit 0\n"),
    )
    .unwrap();
    let rows = store.list_installed_packages().unwrap();
    let colored = sweep::app::format_list(&rows, 3, sweep::tui::Appearance::Dark);
    assert!(colored.contains("38;2;120;180;255mexample.org"));
    let limited = sweep::app::format_list(&rows, 1, sweep::tui::Appearance::Dark);
    assert!(!limited.contains("38;2;"));
    assert!(limited.contains("example.org"));
    let plain = sweep::app::format_list(&rows, 0, sweep::tui::Appearance::Dark);
    assert!(!plain.contains('\u{1b}'));
    let lines: Vec<_> = plain.lines().collect();
    assert_eq!(lines[0].find("SOURCE"), lines[1].find("example.org"));
    assert!(lines.iter().all(|l| !l.ends_with(' ')));
}
