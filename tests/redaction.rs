use sweep::{parse::parse_install_command, redact::redact_command};

#[test]
fn redaction_covers_fetcher_credentials_without_hiding_public_flags() {
    for (raw, expected) in [
        (
            "wget --auth-no-challenge --password private https://example.com/i | sh",
            "wget --auth-no-challenge --password <redacted> https://example.com/i | sh",
        ),
        (
            "curl -u 'name:secret' -H 'Authorization: Bearer private' -H 'Accept: text/plain' https://example.com/i | sh --author Alice",
            "curl -u <redacted> -H <redacted> -H 'Accept: text/plain' https://example.com/i | sh --author Alice",
        ),
        (
            "curl --user=name:secret --header='Proxy-Authorization: Basic private' https://example.com/i | sh",
            "curl --user=<redacted> --header=<redacted> https://example.com/i | sh",
        ),
        (
            "bash <(curl -uname:secret -H'Authorization: Bearer private' https://example.com/i)",
            "bash <(curl -u<redacted> -H<redacted> https://example.com/i)",
        ),
        (
            "curl '--user=alice:secret' '-HAuthorization: Bearer secret' https://example.com/i | sh",
            "curl <redacted> <redacted> https://example.com/i | sh",
        ),
        (
            "curl -fsSu alice:secret -fsSuother:secret https://example.com/i | sh",
            "curl -fsSu <redacted> -fsSu<redacted> https://example.com/i | sh",
        ),
        (
            "wget --http-password=private --header='X-Api-Key: private' https://example.com/i | bash",
            "wget --http-password=<redacted> --header=<redacted> https://example.com/i | bash",
        ),
    ] {
        assert_eq!(
            redact_command(&parse_install_command(raw).unwrap()),
            expected
        );
    }
}

#[test]
fn redaction_consumes_whole_shell_values_including_concatenation_and_escapes() {
    for (raw, expected) in [
        (
            "curl https://example.com/i | sh --token 'first'\"second part\"tail --author Alice",
            "curl https://example.com/i | sh --token <redacted> --author Alice",
        ),
        (
            "curl https://example.com/i | sh --token=bare' secret' --author Alice",
            "curl https://example.com/i | sh --token=<redacted> --author Alice",
        ),
        (
            r#"curl https://example.com/i | sh --token="first\" secret" --author Alice"#,
            "curl https://example.com/i | sh --token=<redacted> --author Alice",
        ),
        (
            "curl -H 'Authorization: Bearer '\"private suffix\" https://example.com/i | sh",
            "curl -H <redacted> https://example.com/i | sh",
        ),
    ] {
        assert_eq!(
            redact_command(&parse_install_command(raw).unwrap()),
            expected
        );
    }
}

#[test]
fn redaction_retains_compact_secret_names() {
    let raw = "MYTOKEN=private ApiKey=private curl https://example.com/i | sh --accessToken private --author=Alice";
    // Mixed-case assignment parsing is a separate parser limitation.
    let mut cmd = parse_install_command(
        "curl https://example.com/i | sh --accessToken private --author=Alice",
    )
    .unwrap();
    cmd.raw = raw.into();
    cmd.env_vars.insert("MYTOKEN".into(), "private".into());
    cmd.env_vars.insert("ApiKey".into(), "private".into());
    assert_eq!(
        redact_command(&cmd),
        "MYTOKEN=<redacted> ApiKey=<redacted> curl https://example.com/i | sh --accessToken <redacted> --author=Alice"
    );
}

#[test]
fn private_keys_and_dash_prefixed_fetcher_credentials_are_hidden() {
    for (raw, expected) in [
        (
            "PRIVATEKEY=private curl https://example.com/i | sh --privateKey private --author Alice",
            "PRIVATEKEY=<redacted> curl https://example.com/i | sh --privateKey <redacted> --author Alice",
        ),
        (
            "curl --user '-alice:private' https://example.com/i | sh",
            "curl --user <redacted> https://example.com/i | sh",
        ),
    ] {
        assert_eq!(
            redact_command(&parse_install_command(raw).unwrap()),
            expected
        );
    }
}
