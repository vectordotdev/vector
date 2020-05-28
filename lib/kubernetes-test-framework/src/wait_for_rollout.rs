use super::Result;
use std::{
    ffi::OsStr,
    process::{Command, Stdio},
};

pub fn run<CMD, NS, R, EX>(
    kubectl_command: CMD,
    namespace: NS,
    resource: R,
    extra: impl IntoIterator<Item = EX>,
) -> Result<()>
where
    CMD: AsRef<OsStr>,
    NS: AsRef<OsStr>,
    R: AsRef<OsStr>,
    EX: AsRef<OsStr>,
{
    let mut command = Command::new(kubectl_command);

    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    command.arg("rollout").arg("status");
    command.arg("-n").arg(namespace);
    command.arg(resource);
    command.args(extra);

    let mut child = command.spawn()?;
    let exit_status = child.wait()?;
    if !exit_status.success() {
        Err(format!("waiting for rollout failed: {:?}", command))?;
    }
    Ok(())
}
