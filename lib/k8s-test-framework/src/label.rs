//! Apply one or more labels to a resource

use super::Result;
use crate::util::run_command;
use std::{ffi::OsStr, process::Stdio};
use tokio::process::Command;

/// Apply one or more labels to a `resource` within
/// a `namespace` to complete via the specified `kubectl_command`.
pub async fn run<Cmd, NS, R, EX>(
    kubectl_command: Cmd,
    namespace: NS,
    resource: R,
    label: L,
) -> Result<()>
where
    Cmd: AsRef<OsStr>,
    NS: AsRef<OsStr>,
    R: AsRef<OsStr>,
    L: AsRef<OsStr>,
{
    let mut command = Command::new(kubectl_command);

    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    command.arg("label");
    command.arg("-n").arg(namespace);
    command.arg(resource);
    command.args(label);

    run_command(command).await?;
    Ok(())
}
