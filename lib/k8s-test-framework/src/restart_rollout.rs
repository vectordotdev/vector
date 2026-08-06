//! Restart a resource rollout.

use std::{ffi::OsStr, process::Stdio};

use tokio::process::Command;

use super::Result;
use crate::util::run_command;

/// Restart a rollout of a `resource` within a `namespace` to complete
/// via the specified `kubectl_command`.
/// Use the `extra` field to pass additional args to `kubectl`
pub async fn run<Cmd, NS, R, EX>(
    kubectl_command: Cmd,
    namespace: NS,
    resource: R,
    extra: impl IntoIterator<Item = EX>,
) -> Result<()>
where
    Cmd: AsRef<OsStr>,
    NS: AsRef<OsStr>,
    R: AsRef<OsStr>,
    EX: AsRef<OsStr>,
{
    let mut command = Command::new(kubectl_command);

    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    command.arg("rollout").arg("restart");
    command.arg("-n").arg(namespace);
    command.arg(resource);
    command.args(extra);

    run_command(command).await?;
    Ok(())
}
