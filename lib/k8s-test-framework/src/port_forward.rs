//! Perform a port forward from a port listening on a local system to the
//! a port exposed from a cluster-deployed resource.

use super::Result;
use std::process::{ExitStatus, Stdio};
use tokio::process::{Child, Command};

/// Initiate a port forward (`kubectl port-forward`) with the specified
/// `kubectl_command` for the specified `resource` at the specified `namespace`
/// and the specified local/cluster-resource ports pair.
/// Returns a [`PortForwarder`] that manages the process.
pub fn port_forward(
    kubectl_command: &str,
    namespace: &str,
    resource: &str,
    local_port: u16,
    resource_port: u16,
) -> Result<PortForwarder> {
    let mut command = Command::new(kubectl_command);

    command
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit());

    command.arg("port-forward");
    command.arg("-n").arg(namespace);
    command.arg(resource);
    command.arg(format!("{}:{}", local_port, resource_port));

    command.kill_on_drop(true);

    let child = command.spawn()?;
    Ok(PortForwarder {
        child,
        local_port,
        resource_port,
    })
}

/// Keeps track of the continiously running `kubectl port-forward` command,
/// exposing the API to terminate it when needed.
#[derive(Debug)]
pub struct PortForwarder {
    child: Child,
    local_port: u16,
    resource_port: u16,
}

impl PortForwarder {
    /// Returns the local port that port forward was requested to listen on.
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    /// Returns the resource port that port forward was requested to forward to.
    pub fn resource_port(&self) -> u16 {
        self.resource_port
    }

    /// Wait for the `kubectl port-forward` process to exit and return the exit
    /// code.
    pub async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        (&mut self.child).await
    }

    /// Send a termination signal to the `kubectl port-forward` process.
    pub fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill()
    }
}
