//! Perform a log lookup.

use std::process::Stdio;

use tokio::process::Command;

use super::{Reader, Result};

/// Initiate a log lookup (`kubectl log`) with the specified `kubectl_command`
/// for the specified `resource` at the specified `namespace`.
/// Returns a [`Reader`] that manages the reading process.
pub fn log_lookup(kubectl_command: &str, namespace: &str, resource: &str) -> Result<Reader> {
    let mut command = Command::new(kubectl_command);

    command.stdin(Stdio::null()).stderr(Stdio::inherit());

    command.arg("logs");
    command.arg("-f");
    command.arg("-n").arg(namespace);
    command.arg(resource);

    let reader = Reader::spawn(command)?;
    Ok(reader)
}
