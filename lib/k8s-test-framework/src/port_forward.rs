//! Perform a port forward from a port listening on a local system to the
//! a port exposed from a cluster-deployed resource.

#![allow(clippy::print_stdout)] // test framework

use std::process::{ExitStatus, Stdio};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, ChildStdout, Command},
};

use super::Result;

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
        .stdout(Stdio::piped());

    command.arg("port-forward");
    command.arg("-n").arg(namespace);
    command.arg(resource);
    command.arg(format!("{}:{}", local_port, resource_port));

    command.kill_on_drop(true);

    let mut child = command.spawn()?;
    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);

    Ok(PortForwarder {
        local_port,
        resource_port,
        child,
        reader,
    })
}

/// Keeps track of the continuously running `kubectl port-forward` command,
/// exposing the API to terminate it when needed.
#[derive(Debug)]
pub struct PortForwarder {
    local_port: u16,
    resource_port: u16,
    child: Child,
    reader: BufReader<ChildStdout>,
}

impl PortForwarder {
    /// Waits for port forward process to start listening on IPv4 and IPv6 local
    /// sockets.
    pub async fn wait_until_ready(&mut self) -> Result<()> {
        let ready_string_ipv4 = format!(
            "Forwarding from 127.0.0.1:{} -> {}",
            self.local_port, self.resource_port
        );
        let ready_string_ipv6 = format!(
            "Forwarding from [::1]:{} -> {}",
            self.local_port, self.resource_port
        );

        let mut buf = String::new();
        let mut seen_ipv4 = false;
        let mut seen_ipv6 = false;
        loop {
            self.reader.read_line(&mut buf).await?;
            print!("{}", &buf);

            if buf.contains(&ready_string_ipv4) {
                seen_ipv4 = true;
            }
            if buf.contains(&ready_string_ipv6) {
                seen_ipv6 = true;
            }

            buf.clear();

            if seen_ipv4 && seen_ipv6 {
                break;
            }
        }
        Ok(())
    }

    /// Returns the local port that port forward was requested to listen on.
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    /// Returns the resource port that port forward was requested to forward to.
    pub fn resource_port(&self) -> u16 {
        self.resource_port
    }

    /// Returns the local address (in the "host:port" form) to connect to
    /// in order to reach the cluster resource port, at the IPv4 address family.
    pub fn local_addr_ipv4(&self) -> String {
        format!("127.0.0.1:{}", self.local_port)
    }

    /// Returns the local address (in the "host:port" form) to connect to
    /// in order to reach the cluster resource port, at the IPv6 address family.
    pub fn local_addr_ipv6(&self) -> String {
        format!("[::1]:{}", self.local_port)
    }

    /// Wait for the `kubectl port-forward` process to exit and return the exit
    /// code.
    pub async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child.wait().await
    }

    /// Send a termination signal to the `kubectl port-forward` process.
    pub async fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill().await
    }
}
