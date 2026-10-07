use sweep::{exec::run_script, parse::parse_install_command};

#[test]
fn startup_environment_is_rejected_before_running_any_bytes() {
    let home = tempfile::tempdir().unwrap();
    let marker = home.path().join("startup-ran");
    let raw = format!(
        "BASH_ENV='$(echo unexpected > {})' curl https://example.com/install | bash",
        marker.display()
    );
    let cmd = parse_install_command(&raw).unwrap();
    let result = run_script(&cmd, b"true\n");
    assert!(result.is_err(), "startup environment must be rejected");
    assert!(!marker.exists(), "startup executed before reviewed bytes");
}

#[test]
fn command_path_cannot_replace_the_runner() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let shell = home.path().join("bash");
    std::fs::write(&shell, "#!/bin/sh\nexit 3\n").unwrap();
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cmd = parse_install_command(&format!(
        "PATH={} curl https://example.com/install | bash",
        home.path().display()
    ))
    .unwrap();
    assert_eq!(run_script(&cmd, b"exit 0\n").unwrap(), 0);
}

#[test]
fn alternate_startup_and_loader_overrides_are_rejected() {
    for name in [
        "ENV",
        "ZDOTDIR",
        "SHELLOPTS",
        "BASHOPTS",
        "PS4",
        "PROMPT_COMMAND",
        "LD_PRELOAD",
        "LD_LIBRARY_PATH",
        "LD_AUDIT",
        "DYLD_INSERT_LIBRARIES",
        "DYLD_LIBRARY_PATH",
        "BASH_FUNC_startup%%",
    ] {
        let mut cmd = parse_install_command("curl https://example.com/install | sh").unwrap();
        cmd.env_vars.insert(name.into(), String::new());
        let error = run_script(&cmd, b"true\n")
            .expect_err("startup or loader override must not reach the child");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{name}");
        assert!(error.to_string().contains(name), "{name}");
    }
}

#[test]
fn zsh_does_not_source_startup_from_an_overridden_home() {
    if !["/bin/zsh", "/usr/bin/zsh"]
        .iter()
        .any(|path| std::path::Path::new(path).exists())
    {
        return;
    }
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join(".zshenv"), "exit 3\n").unwrap();
    let cmd = parse_install_command(&format!(
        "HOME={} curl https://example.com/install | zsh",
        home.path().display()
    ))
    .unwrap();
    assert_eq!(run_script(&cmd, b"exit 0\n").unwrap(), 0);
}
