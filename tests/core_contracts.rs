use sweep::{
    parse::{parse_install_command, slug_from_url},
    redact::redact_command,
};
#[test]
fn parser_preserves_command_contract_and_refusals() {
    let raw = "  TOKEN='a && b' VERSION=2 /usr/bin/curl -f 'https://example.com/i?q=1&x=2' | sudo -E /bin/bash -s -- --to /tmp/bin  ";
    let c = parse_install_command(raw).unwrap();
    assert_eq!(c.raw, raw);
    assert_eq!(c.env_vars["TOKEN"], "a && b");
    assert_eq!(c.env_vars["VERSION"], "2");
    assert!(c.sudo);
    assert_eq!(c.shell, "bash");
    assert_eq!(c.script_args, ["--to", "/tmp/bin"]);
    assert_eq!(c.url, "https://example.com/i?q=1&x=2");
    for (input, kind) in [
        (" ", "empty"),
        ("curl https://a | sh && true", "chain"),
        ("cat /tmp/x | sh", "no-fetcher"),
        ("curl https://a", "no-pipe"),
        ("curl | sh", "no-url"),
        ("curl https://a | fish", "unsupported"),
        ("curl https://a | tee /tmp/x | sh", "unsupported"),
    ] {
        assert_eq!(
            parse_install_command(input).unwrap_err().kind,
            kind,
            "{input}"
        );
    }
    for input in [
        "/bin/bash -c \"$(curl -fsSL https://example.com/i)\"",
        "sudo bash <(wget -qO- https://example.com/i)",
    ] {
        let c = parse_install_command(input).unwrap();
        assert_eq!(c.url, "https://example.com/i");
        assert_eq!(c.shell, "bash");
    }
}
#[test]
fn corpus_matches_reference_acceptance() {
    for line in include_str!("fixtures/parser-reference.jsonl").lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let result = parse_install_command(row["input"].as_str().unwrap());
        match row.get("error") {
            Some(kind) => assert_eq!(result.unwrap_err().kind, kind.as_str().unwrap()),
            None => assert_eq!(
                serde_json::to_value(result.unwrap()).unwrap(),
                row["parsed"]
            ),
        }
    }
}
#[test]
fn naming_strips_only_one_delivery_prefix() {
    for (url, slug) in [
        ("https://Get.Bun.SH", "bun"),
        ("https://www.get.bun.sh", "get"),
        ("https://cdn.example.com/i", "example"),
        ("http://192.168.1.1/i", "192"),
        ("file:///tmp/x", "unknown"),
        ("invalid", "unknown"),
    ] {
        assert_eq!(slug_from_url(url), slug);
    }
}
#[test]
fn redaction_is_anchor_based_and_preserves_every_other_byte() {
    let raw = "API_KEY=one API_KEY='two words' VERSION=one curl https://example.com/i | sh -s -- --token  abc --token=def --auth \"Bearer ghi\" --password --other";
    let c = parse_install_command(raw).unwrap();
    assert_eq!(
        redact_command(&c),
        "API_KEY=<redacted> API_KEY=<redacted> VERSION=one curl https://example.com/i | sh -s -- --token  <redacted> --token=<redacted> --auth <redacted> --password --other"
    );
    let raw = "GITHUB_PAT=abc curl https://example.com/i?token=x | sh -s -- -t a --token -dash";
    assert_eq!(redact_command(&parse_install_command(raw).unwrap()), raw);
}
#[test]
fn executor_passes_bytes_args_environment_and_exit_status() {
    let home = tempfile::tempdir().unwrap();
    let output = home.path().join("result");
    let mut cmd =
        parse_install_command("curl https://example.com/i | sh -s -- first second").unwrap();
    cmd.env_vars
        .insert("OUT".into(), output.display().to_string());
    cmd.env_vars
        .insert("SWEEP_TEST_VALUE".into(), "literal $thing".into());
    let code = sweep::exec::run_script(
        &cmd,
        b"printf '%s|%s|%s' \"$1\" \"$2\" \"$SWEEP_TEST_VALUE\" > \"$OUT\"\nexit 3\n",
    )
    .unwrap();
    assert_eq!(code, 3);
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        "first|second|literal $thing"
    );
    cmd.shell = "/definitely/nonexistent/sweep-shell".into();
    assert!(sweep::exec::run_script(&cmd, b"true\n").is_err());
}
fn http_fixture(responses: Vec<Vec<u8>>) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for response in responses {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let _ = socket.read(&mut request);
            let _ = socket.write_all(&response);
        }
    });
    url
}
#[tokio::test]
async fn fetch_follows_redirect_and_hashes_exact_bytes() {
    use sha2::{Digest, Sha256};
    let body = b"#!/bin/sh\ntrue\n\xff";
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend(body);
    let url = http_fixture(vec![
        b"HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_vec(),
        response,
    ]);
    let fetched = sweep::fetch::fetch_script(&url, &Default::default())
        .await
        .unwrap();
    assert_eq!(fetched.bytes, body);
    assert_eq!(
        fetched.sha256,
        Sha256::digest(body)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    assert_eq!(fetched.final_url, format!("{url}/final"));
    assert_eq!(fetched.status, 200);
    assert!(chrono::DateTime::parse_from_rfc3339(&fetched.fetched_at).is_ok());
}
#[tokio::test]
async fn fetch_classifies_http_size_cancel_and_body_timeout() {
    use std::time::Duration;
    use sweep::fetch::{fetch_script, fetch_script_with_timeout};
    for (response, reason) in [
        (
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n",
            "non-2xx",
        ),
        (
            "HTTP/1.1 200 OK\r\nContent-Length: 5242881\r\n\r\n",
            "too-large",
        ),
    ] {
        let url = http_fixture(vec![response.as_bytes().to_vec()]);
        assert_eq!(
            fetch_script(&url, &Default::default())
                .await
                .unwrap_err()
                .reason,
            reason
        );
    }
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    assert_eq!(
        fetch_script("http://127.0.0.1:1", &cancel)
            .await
            .unwrap_err()
            .reason,
        "timeout"
    );
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut b = [0; 2048];
        let _ = stream.read(&mut b);
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nx");
        std::thread::sleep(Duration::from_millis(200));
    });
    assert_eq!(
        fetch_script_with_timeout(&url, &Default::default(), Duration::from_millis(40))
            .await
            .unwrap_err()
            .reason,
        "timeout"
    );
    assert_eq!(
        fetch_script("http://127.0.0.1:1", &Default::default())
            .await
            .unwrap_err()
            .reason,
        "network"
    );
}
#[test]
fn store_preserves_legacy_schema_and_atomic_package_lifecycle() {
    use sweep::store::{Invocation, Store};
    let home = tempfile::tempdir().unwrap();
    let db = rusqlite::Connection::open(home.path().join("sweep.db")).unwrap();
    db.execute_batch("CREATE TABLE schema_meta(version INTEGER PRIMARY KEY); INSERT INTO schema_meta VALUES (1);CREATE TABLE packages(id INTEGER PRIMARY KEY,slug TEXT NOT NULL,source_url TEXT NOT NULL UNIQUE,current_sha256 TEXT,status TEXT NOT NULL,first_seen_at TEXT NOT NULL,installed_at TEXT,last_ran_at TEXT);INSERT INTO packages VALUES(42,'legacy','https://legacy.example/i','old','installed','2024-01-01','2024-01-02','2024-01-03');").unwrap();
    let mut store = Store::open(home.path()).unwrap();
    let legacy = store
        .find_or_create_package("https://legacy.example/i", "changed")
        .unwrap();
    assert_eq!(legacy.id, 42);
    assert_eq!(legacy.slug, "legacy");
    assert_eq!(store.list_installed_packages().unwrap().len(), 1);
    let pkg = store
        .find_or_create_package("https://example.com/i", "example")
        .unwrap();
    assert_eq!(pkg.status, "attempting");
    let mut inv = Invocation {
        id: "attempt-1".into(),
        package_id: Some(pkg.id),
        ts_started: "2026-01-01".into(),
        raw_input: "curl https://example.com/i | sh".into(),
        outcome: "ran".into(),
        exit_code: Some(0),
        ..Default::default()
    };
    store
        .record_exec(&inv, pkg.id, "first", 0, "2026-01-01")
        .unwrap();
    inv.id = "attempt-2".into();
    inv.outcome = "errored".into();
    inv.exit_code = Some(3);
    store
        .record_exec(&inv, pkg.id, "bad", 3, "2026-01-02")
        .unwrap();
    let installed = store
        .find_or_create_package("https://example.com/i", "other")
        .unwrap();
    assert_eq!(installed.status, "installed");
    assert_eq!(installed.current_sha256.as_deref(), Some("first"));
    assert_eq!(installed.installed_at.as_deref(), Some("2026-01-01"));
    assert_eq!(installed.last_ran_at.as_deref(), Some("2026-01-02"));
    assert!(
        store
            .record_exec(&inv, pkg.id, "should-rollback", 0, "2026-01-03")
            .is_err()
    );
    assert_eq!(
        store
            .find_or_create_package("https://example.com/i", "x")
            .unwrap()
            .current_sha256
            .as_deref(),
        Some("first")
    );
    let count: i64 = db
        .query_row("SELECT count(*) FROM invocations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
    let fresh = store
        .find_or_create_package("https://failure.example/i", "failure")
        .unwrap();
    inv.id = "attempt-3".into();
    inv.package_id = Some(fresh.id);
    store
        .record_exec(&inv, fresh.id, "bad", 3, "2026-01-03")
        .unwrap();
    assert_eq!(
        store
            .find_or_create_package("https://failure.example/i", "x")
            .unwrap()
            .status,
        "failed"
    );
}
#[test]
fn script_store_is_binary_exact_deduplicated_and_home_scoped() {
    use sha2::{Digest, Sha256};
    let home = tempfile::tempdir().unwrap();
    let store = sweep::store::Store::open(home.path()).unwrap();
    let bytes = b"\xff\x00true\n";
    let sha = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(store.read_script(&sha).unwrap(), None);
    let path = store.save_script(&sha, bytes).unwrap();
    assert_eq!(path, home.path().join("cache/scripts").join(&sha));
    assert_eq!(store.read_script(&sha).unwrap().unwrap(), bytes);
    assert_eq!(store.save_script(&sha, bytes).unwrap(), path);
    assert!(store.save_script("../../escape", bytes).is_err());
}
#[tokio::test]
async fn streaming_limit_applies_without_content_length_and_cancel_interrupts_body() {
    use std::io::{Read, Write};
    use std::time::Duration;
    let mut response = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
    response.resize(response.len() + 5 * 1024 * 1024 + 1, b'x');
    let url = http_fixture(vec![response]);
    assert_eq!(
        sweep::fetch::fetch_script(&url, &Default::default())
            .await
            .unwrap_err()
            .reason,
        "too-large"
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = tokio_util::sync::CancellationToken::new();
    let trigger = cancel.clone();
    std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut b = [0; 2048];
        let _ = s.read(&mut b);
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nx");
        std::thread::sleep(Duration::from_millis(20));
        trigger.cancel();
        std::thread::sleep(Duration::from_millis(100));
    });
    assert_eq!(
        sweep::fetch::fetch_script(&url, &cancel)
            .await
            .unwrap_err()
            .reason,
        "timeout"
    );
}
#[test]
fn invocations_round_trip_nullable_and_populated_fields_with_foreign_keys() {
    use sweep::store::{Invocation, Store};
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let p = store
        .find_or_create_package("https://example.com/i", "example")
        .unwrap();
    let command =
        parse_install_command("VERSION=2 curl https://example.com/i | bash -s -- --flag").unwrap();
    let inv = Invocation {
        id: "complete".into(),
        package_id: Some(p.id),
        ts_started: "2026-01-01T01:00:00.000Z".into(),
        ts_finished: Some("2026-01-01T01:00:01.000Z".into()),
        raw_input: command.raw.clone(),
        url: Some(command.url.clone()),
        final_url: Some("https://cdn.example.com/i".into()),
        sha256: Some("abc".into()),
        install_command_json: Some(serde_json::to_string(&command).unwrap()),
        outcome: "errored".into(),
        exit_code: Some(3),
        error_message: Some("child failed".into()),
    };
    store.insert_invocation(&inv).unwrap();
    let db = rusqlite::Connection::open(home.path().join("sweep.db")).unwrap();
    let tuple:(String,String,String,i32)=db.query_row("SELECT raw_input,final_url,install_command_json,exit_code FROM invocations WHERE id='complete'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(tuple.0, command.raw);
    assert_eq!(tuple.1, "https://cdn.example.com/i");
    assert_eq!(
        serde_json::from_str::<sweep::parse::InstallCommand>(&tuple.2).unwrap(),
        command
    );
    assert_eq!(tuple.3, 3);
    let mut failed = Invocation {
        id: "parse".into(),
        ts_started: "2026-01-02".into(),
        raw_input: "hello".into(),
        outcome: "parse_failed".into(),
        ..Default::default()
    };
    store.insert_invocation(&failed).unwrap();
    let nulls:bool=db.query_row("SELECT package_id IS NULL AND url IS NULL AND sha256 IS NULL AND exit_code IS NULL FROM invocations WHERE id='parse'",[],|r|r.get(0)).unwrap();
    assert!(nulls);
    failed.id = "bogus-fk".into();
    failed.package_id = Some(99999);
    assert!(store.insert_invocation(&failed).is_err());
    drop(store);
    let reopened = Store::open(home.path()).unwrap();
    assert_eq!(
        reopened
            .find_or_create_package("https://example.com/i", "changed")
            .unwrap()
            .id,
        p.id
    );
}
#[test]
fn reinstallation_preserves_first_success_and_listing_order() {
    use sweep::store::{Invocation, Store};
    let home = tempfile::tempdir().unwrap();
    let mut store = Store::open(home.path()).unwrap();
    let a = store
        .find_or_create_package("https://a.example/i", "a")
        .unwrap();
    let b = store
        .find_or_create_package("https://b.example/i", "b")
        .unwrap();
    for (id, p, sha, date) in [
        ("a-first", a.id, "sha1", "2026-01-01"),
        ("b-first", b.id, "sha2", "2026-01-02"),
        ("a-again", a.id, "sha3", "2026-01-03"),
    ] {
        let inv = Invocation {
            id: id.into(),
            package_id: Some(p),
            ts_started: date.into(),
            raw_input: "true".into(),
            outcome: "ran".into(),
            exit_code: Some(0),
            ..Default::default()
        };
        store.record_exec(&inv, p, sha, 0, date).unwrap();
    }
    let rows = store.list_installed_packages().unwrap();
    assert_eq!(
        rows.iter().map(|p| p.slug.as_str()).collect::<Vec<_>>(),
        ["b", "a"]
    );
    assert_eq!(rows[1].current_sha256.as_deref(), Some("sha3"));
    assert_eq!(rows[1].installed_at.as_deref(), Some("2026-01-01"));
}
#[test]
fn redaction_handles_keyword_case_overlap_empty_and_unicode_values() {
    for name in [
        "API_KEY",
        "TOKEN",
        "SECRET",
        "PASS",
        "AUTH",
        "CRED",
        "Api_Token",
    ] {
        let mut cmd = parse_install_command("curl https://example.com/i | sh").unwrap();
        cmd.raw = format!("prefix é {name}='密 密' --token= --token='second secret'");
        cmd.env_vars.insert(name.into(), "密 密".into());
        cmd.script_args = vec!["--token=".into(), "--token=second".into()];
        assert_eq!(
            redact_command(&cmd),
            format!("prefix é {name}=<redacted> --token=<redacted> --token=<redacted>")
        );
    }
    let mut cmd =
        parse_install_command("TOKEN='KEY=inner secret' curl https://example.com/i | sh").unwrap();
    cmd.env_vars.insert("KEY".into(), "inner".into());
    assert_eq!(
        redact_command(&cmd),
        "TOKEN=<redacted> curl https://example.com/i | sh"
    );
}
#[test]
fn executor_handles_early_exit_and_inherits_parent_environment() {
    let mut cmd = parse_install_command("curl https://example.com/i | bash").unwrap();
    cmd.env_vars
        .insert("HOME".into(), "/sweep-test-override".into());
    assert_eq!(
        sweep::exec::run_script(
            &cmd,
            b"test \"$HOME\" = /sweep-test-override && test -n \"$PATH\"\n"
        )
        .unwrap(),
        0
    );
    let mut bytes = b"exit 7\n".to_vec();
    bytes.resize(1024 * 1024, b' ');
    assert_eq!(sweep::exec::run_script(&cmd, &bytes).unwrap(), 7);
}
#[test]
fn concurrent_cas_publication_never_exposes_partial_script() {
    use sha2::{Digest, Sha256};
    use std::sync::{Arc, Barrier};
    let home = tempfile::tempdir().unwrap();
    let bytes = Arc::new(vec![0xff; 2 * 1024 * 1024]);
    let sha = hex::encode(Sha256::digest(bytes.as_slice()));
    let barrier = Arc::new(Barrier::new(5));
    let mut workers = vec![];
    for _ in 0..4 {
        let path = home.path().to_owned();
        let bytes = bytes.clone();
        let sha = sha.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            let store = sweep::store::Store::open(&path).unwrap();
            barrier.wait();
            store.save_script(&sha, &bytes).unwrap();
        }));
    }
    barrier.wait();
    let path = home.path().join("cache/scripts").join(sha);
    loop {
        match std::fs::read(&path) {
            Ok(actual) => {
                assert_eq!(actual, *bytes);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::thread::yield_now(),
            Err(e) => panic!("{e}"),
        }
    }
    for worker in workers {
        worker.join().unwrap();
    }
}
#[tokio::test]
async fn fetch_decodes_compressed_installer_before_hashing_or_execution() {
    let compressed = [
        31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 83, 86, 212, 79, 202, 204, 211, 47, 206, 224, 42, 41,
        42, 77, 229, 2, 0, 61, 54, 132, 171, 15, 0, 0, 0,
    ];
    let mut response=format!("HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",compressed.len()).into_bytes();
    response.extend(compressed);
    let url = http_fixture(vec![response]);
    let fetched = sweep::fetch::fetch_script(&url, &Default::default())
        .await
        .unwrap();
    assert_eq!(fetched.bytes, b"#!/bin/sh\ntrue\n");
    use sha2::{Digest, Sha256};
    assert_eq!(
        fetched.sha256,
        hex::encode(Sha256::digest(b"#!/bin/sh\ntrue\n"))
    );
}
