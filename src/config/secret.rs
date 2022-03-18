use std::{collections::HashMap, io::Error};

use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command};
use typetag::serde;
use tokio::time::Duration;

#[typetag::serde(tag = "type")]
pub trait SecretBackend: core::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    fn retrieve(&mut self, secret_keys: Vec<String>) -> HashMap<String, crate::Result<String>>;
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct ExecBackend {
    pub command: Vec<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout: u64,
}

const fn default_timeout_secs() -> u64 {
    5
}

///////////////////////////////////////////////////////////
// serde_json::to_vec

#[derive(Deserialize, Serialize, Debug, Clone)]
struct ExecQuery {
    version: String,
    secrets: Vec<String>,
}

fn new_query(secrets: Vec<String>) -> ExecQuery {
    ExecQuery {
        version: "1.0".to_string(),
        secrets,
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct ExecResponse {
    value: Option<String>,
    error: Option<String>,
}

#[typetag::serde(name = "exec")]
impl SecretBackend for ExecBackend {
    fn retrieve(&mut self, secret_keys: Vec<String>) -> HashMap<String, crate::Result<String>> {
        let timeout = tokio::time::timeout(
            Duration::from_secs(self.timeout),
            query_backend(&self.command, new_query(secret_keys)),
        );

        let timeout_result = timeout.await;
        return HashMap::new();
    }
}

async fn query_backend(cmd: &Vec<String>, query: ExecQuery) -> crate::Result<ExecResponse> {
    let command = &cmd[0];
    let mut command = Command::new(command);

    if cmd.len() > 1 {
        command.args(&cmd[1..]);
    };
    command.kill_on_drop(true);
    // Pipe stdin/stdout/stderr
    command.stderr(std::process::Stdio::piped());
    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());

    let mut child = command.spawn()?;
    let mut stdin = child.stdin.take().ok_or("unable to acquire stdin")?;

    let query = serde_json::to_vec(&query)?;
    stdin.write_all(&query).await?;

    let output = child.wait_with_output().await?;
    let response = serde_json::from_slice::<ExecResponse>(&output.stdout)?;

    Ok(response)
}
