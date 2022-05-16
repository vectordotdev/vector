use std::{ffi::OsStr, process::Stdio};

use tokio::process::Command;

use super::Result;
use crate::util::run_command_output;

#[derive(Debug)]
pub struct CommandBuilder {}

fn prepare_base_command<Cmd, NS, Pod>(
    kubectl_command: Cmd,
    namespace: NS,
    pod: Option<Pod>,
) -> Command
where
    Cmd: AsRef<OsStr>,
    NS: AsRef<OsStr>,
    Pod: AsRef<OsStr>,
{
    let mut command = Command::new(&kubectl_command);

    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    command.arg("get").arg("pod");

    if let Some(pod) = pod {
        command.arg(pod);
    }

    command.arg("-n").arg(namespace).arg("-o").arg("json");

    command
}

/// Returns the node the given pod is on
pub async fn get_node<Cmd, NS, Pod>(kubectl_command: Cmd, namespace: NS, pod: Pod) -> Result<String>
where
    Pod: AsRef<OsStr>,
    Cmd: AsRef<OsStr>,
    NS: AsRef<OsStr>,
{
    let command = prepare_base_command(kubectl_command, namespace, Some(pod));
    let pod = run_command_output(command).await?;
    let pod: serde_json::Value = serde_json::from_str(&pod)?;

    let node = pod["spec"]["nodeName"]
        .as_str()
        .ok_or("nodename must be a string")?;

    Ok(node.to_string())
}

/// Set label on all nodes to discover this label from the pod.
pub async fn label_nodes<Cmd, Label>(kubectl_command: Cmd, label: Label) -> Result<String>
where
    Cmd: AsRef<OsStr>,
    Label: AsRef<OsStr>,
{
    let mut command = Command::new(&kubectl_command);
    command
        .arg("label")
        .arg("node")
        .arg(label)
        .arg("--all")
        .arg("--overwrite");

    let res = run_command_output(command).await?;
    Ok(res)
}

pub async fn get_pod_on_node<Cmd, NS>(
    kubectl_command: Cmd,
    namespace: NS,
    node: &str,
    service: &str,
) -> Result<String>
where
    Cmd: AsRef<OsStr>,
    NS: AsRef<OsStr>,
{
    let nopod: Option<&str> = None;
    let command = prepare_base_command(kubectl_command, namespace, nopod);
    let pods = run_command_output(command).await?;
    let pods: serde_json::Value = serde_json::from_str(&pods)?;

    let pods = pods["items"].as_array().ok_or("items should be an array")?;

    for pod in pods {
        if pod["spec"]["nodeName"]
            .as_str()
            .ok_or("nodeName must be a string")?
            == node
            && pod["spec"]["serviceAccount"]
                .as_str()
                .ok_or("serviceAccount must be a string")?
                == service
        {
            return Ok(pod["metadata"]["name"]
                .as_str()
                .ok_or("name must be a string")?
                .to_string());
        }
    }

    Err("No pod on this node".into())
}
