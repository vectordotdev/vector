//! Wait for a resource rollout to complete.

use std::{ffi::OsStr, process::Stdio};

use tokio::process::Command;

use super::Result;
use crate::util::run_command;

/// Wait for a rollout of a `resource` within a `namespace` to complete via
/// the specified `kubectl_command`.
/// Use `extra` to pass additional arguments to `kubectl`.
pub async fn run<Cmd, NS, R, Ex>(
    kubectl_command: Cmd,
    namespace: NS,
    resource: R,
    extra: impl IntoIterator<Item = Ex>,
) -> Result<()>
where
    Cmd: AsRef<OsStr>,
    NS: AsRef<OsStr>,
    R: AsRef<OsStr>,
    Ex: AsRef<OsStr>,
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

    run_command(command).await?;
    Ok(())
}
