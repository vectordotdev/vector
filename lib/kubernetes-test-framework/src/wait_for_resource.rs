//! Wait for a resource to reach a certain condition.

use super::Result;
use crate::util::run_command;
use std::{ffi::OsStr, process::Stdio};
use tokio::process::Command;

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
pub async fn namespace<CMD, NS, R, COND, EX>(
    kubectl_command: CMD,
    namespace: NS,
    resources: impl IntoIterator<Item = R>,
    wait_for: WaitFor<COND>,
    extra: impl IntoIterator<Item = EX>,
) -> Result<()>
where
    CMD: AsRef<OsStr>,
    NS: AsRef<OsStr>,
    R: AsRef<OsStr>,
    COND: std::fmt::Display,
    EX: AsRef<OsStr>,
{
    let mut command = prepare_base_command(kubectl_command, resources, wait_for, extra);
    command.arg("-n").arg(namespace);
    run_command(command).await
}

/// Wait for a set of `resources` at any namespace to reach a `wait_for`
/// condition.
/// Use `extra` to pass additional arguments to `kubectl`.
pub async fn all_namespaces<CMD, R, COND, EX>(
    kubectl_command: CMD,
    resources: impl IntoIterator<Item = R>,
    wait_for: WaitFor<COND>,
    extra: impl IntoIterator<Item = EX>,
) -> Result<()>
where
    CMD: AsRef<OsStr>,
    R: AsRef<OsStr>,
    COND: std::fmt::Display,
    EX: AsRef<OsStr>,
{
    let mut command = prepare_base_command(kubectl_command, resources, wait_for, extra);
    command.arg("--all-namespaces=true");
    run_command(command).await
}

fn prepare_base_command<CMD, R, COND, EX>(
    kubectl_command: CMD,
    resources: impl IntoIterator<Item = R>,
    wait_for: WaitFor<COND>,
    extra: impl IntoIterator<Item = EX>,
) -> Command
where
    CMD: AsRef<OsStr>,
    R: AsRef<OsStr>,
    COND: std::fmt::Display,
    EX: AsRef<OsStr>,
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
