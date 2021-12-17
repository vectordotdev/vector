//! Perform a log lookup.

use std::process::Stdio;

use tokio::process::Command;

use super::{Reader, Result};

/// Exec a `tail` command reading the specified `file` within a `Container`
/// in a `Pod` of a specified `resource` at the specified `namespace` via the
/// specified `kubectl_command`.
/// Returns a [`Reader`] that manages the reading process.
pub fn exec_tail(
    kubectl_command: &str,
    namespace: &str,
    resource: &str,
    file: &str,
) -> Result<Reader> {
    let mut command = Command::new(kubectl_command);

    command.stdin(Stdio::null()).stderr(Stdio::inherit());

    command.arg("exec");
    command.arg("-n").arg(namespace);
    command.arg(resource);
    command.arg("--");
    command.arg("tail");
    command.arg("--follow=name");
    command.arg("--retry");
    command.arg(file);

    let reader = Reader::spawn(command)?;
    Ok(reader)
}
