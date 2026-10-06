use crate::parse::InstallCommand;
use std::{
    io::{self, Write},
    process::{Command, Stdio},
};
/// Feed only saved bytes to the selected shell; retain its controlling terminal.
pub fn run_script(cmd: &InstallCommand, bytes: &[u8]) -> io::Result<i32> {
    let mut process = Command::new(if cmd.sudo { "sudo" } else { &cmd.shell });
    if cmd.sudo {
        process.arg(&cmd.shell);
    }
    process
        .args(["-s", "--"])
        .args(&cmd.script_args)
        .envs(&cmd.env_vars)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let mut child = process.spawn()?;
    let written = child.stdin.take().expect("piped stdin").write_all(bytes);
    let status = child.wait()?;
    // Early shell exit closes stdin legitimately; still report its exit status.
    if let Err(error) = written
        && error.kind() != io::ErrorKind::BrokenPipe
    {
        return Err(error);
    }
    Ok(status.code().unwrap_or(-1))
}
