use crate::parse::InstallCommand;
use std::{
    io::{self, Write},
    os::unix::fs::PermissionsExt,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn startup_override(name: &str) -> bool {
    matches!(
        name,
        "BASH_ENV" | "ENV" | "ZDOTDIR" | "SHELLOPTS" | "BASHOPTS" | "PS4" | "PROMPT_COMMAND"
    ) || name.starts_with("LD_")
        || name.starts_with("DYLD_")
        || name.starts_with("BASH_FUNC_")
}

// Resolve using Sweep's environment before installing command-supplied overrides.
// Absolute paths also keep sudo from doing its own PATH lookup for the shell.
fn resolve_program(name: &str) -> io::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    if name.contains('/') {
        return Ok(cwd.join(name));
    }
    let path = std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into());
    std::env::split_paths(&path)
        .map(|directory| cwd.join(directory).join(name))
        .find(|candidate| {
            candidate.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("cannot find {name} in PATH"),
            )
        })
}

/// Feed only saved bytes to the selected shell; retain its controlling terminal.
pub fn run_script(cmd: &InstallCommand, bytes: &[u8]) -> io::Result<i32> {
    let signals = crate::exec_signals::ExecutionSignals::open()?;
    run_script_with_signals(cmd, bytes, &signals)
}

pub(crate) fn run_script_with_signals(
    cmd: &InstallCommand,
    bytes: &[u8],
    signals: &crate::exec_signals::ExecutionSignals,
) -> io::Result<i32> {
    if let Some(name) = cmd.env_vars.keys().find(|name| startup_override(name)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("environment override {name} can run code before the approved script"),
        ));
    }
    let shell = resolve_program(&cmd.shell)?;
    let mut process = Command::new(if cmd.sudo {
        resolve_program("sudo")?
    } else {
        shell.clone()
    });
    if cmd.sudo {
        process.arg(&shell);
    }
    // zsh normally sources $HOME/.zshenv even for non-interactive scripts.
    if Path::new(&cmd.shell)
        .file_name()
        .is_some_and(|name| name == "zsh")
    {
        process.arg("-f");
    }
    process
        .args(["-s", "--"])
        .args(&cmd.script_args)
        .envs(&cmd.env_vars)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    signals.prepare(&mut process);
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            signals.restore_spawn_failure()?;
            return Err(error);
        }
    };
    signals.set_child(child.id());
    let mut input = child.stdin.take().expect("piped stdin");
    let (written, status, restored) = std::thread::scope(|scope| {
        // A stopped installer can fill the pipe. Keep observing its job-control
        // state independently of the potentially blocked input writer.
        let writer = scope.spawn(move || input.write_all(bytes));
        let status = signals.wait(child.id());
        let restored = signals.restore_terminal();
        if status.is_err() || restored.is_err() {
            // A stopped child can keep the writer blocked after a wait/handoff
            // error. Terminate its group and reap it before joining the writer.
            unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) };
            if status.is_err() {
                let _ = child.wait();
            }
        }
        signals.clear_child();
        let written = writer
            .join()
            .map_err(|_| io::Error::other("installer input writer panicked"))?;
        Ok::<_, io::Error>((written, status, restored))
    })?;
    let status = status?;
    restored?;
    // Early shell exit closes stdin legitimately; still report its exit status.
    if let Err(error) = written
        && error.kind() != io::ErrorKind::BrokenPipe
    {
        return Err(error);
    }
    Ok(status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)))
}
