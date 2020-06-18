//! Wait for a resource rollout to complete.

use super::Result;
use crate::util::run_command;
use std::{ffi::OsStr, process::Stdio};
use tokio::process::Command;

/// Wait for a rollout of a `resource` within a `namespace` to complete via
/// the specifed `kubectl_command`.
/// Use `extra` to pass additional arguments to `kubectl`.
pub async fn run<CMD, NS, R, EX>(
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

    run_command(command).await?;
    Ok(())
}
