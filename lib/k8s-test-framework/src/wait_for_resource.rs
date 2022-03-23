//! Wait for a resource to reach a certain condition.

use std::{ffi::OsStr, process::Stdio};

use tokio::process::Command;

use super::Result;
use crate::util::run_command;

/// Specify what condition to wait for.
#[derive(Debug)]
pub enum WaitFor<C>
where
    C: std::fmt::Display,
{
    /// Wait for resource deletion.
    Delete,
    /// Wait for the specified condition.
    Condition(C),
}

/// Wait for a set of `resources` within a `namespace` to reach a `wait_for`
/// condition.
/// Use `extra` to pass additional arguments to `kubectl`.
pub async fn namespace<Cmd, NS, R, Cond, Ex>(
    kubectl_command: Cmd,
    namespace: NS,
    resources: impl IntoIterator<Item = R>,
    wait_for: WaitFor<Cond>,
    extra: impl IntoIterator<Item = Ex>,
) -> Result<()>
where
    Cmd: AsRef<OsStr>,
    NS: AsRef<OsStr>,
    R: AsRef<OsStr>,
    Cond: std::fmt::Display,
    Ex: AsRef<OsStr>,
{
    let mut command = prepare_base_command(kubectl_command, resources, wait_for, extra);
    command.arg("-n").arg(namespace);
    run_command(command).await
}

/// Wait for a set of `resources` at any namespace to reach a `wait_for`
/// condition.
/// Use `extra` to pass additional arguments to `kubectl`.
pub async fn all_namespaces<Cmd, R, Cond, Ex>(
    kubectl_command: Cmd,
    resources: impl IntoIterator<Item = R>,
    wait_for: WaitFor<Cond>,
    extra: impl IntoIterator<Item = Ex>,
) -> Result<()>
where
    Cmd: AsRef<OsStr>,
    R: AsRef<OsStr>,
    Cond: std::fmt::Display,
    Ex: AsRef<OsStr>,
{
    let mut command = prepare_base_command(kubectl_command, resources, wait_for, extra);
    command.arg("--all-namespaces=true");
    run_command(command).await
}

fn prepare_base_command<Cmd, R, Cond, Ex>(
    kubectl_command: Cmd,
    resources: impl IntoIterator<Item = R>,
    wait_for: WaitFor<Cond>,
    extra: impl IntoIterator<Item = Ex>,
) -> Command
where
    Cmd: AsRef<OsStr>,
    R: AsRef<OsStr>,
    Cond: std::fmt::Display,
    Ex: AsRef<OsStr>,
{
    let mut command = Command::new(kubectl_command);

    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    command.arg("wait");
    command.args(resources);

    command.arg("--for");
    match wait_for {
        WaitFor::Delete => command.arg("delete"),
        WaitFor::Condition(cond) => command.arg(format!("condition={}", cond)),
    };

    command.args(extra);
    command
}
