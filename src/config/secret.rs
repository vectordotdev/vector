use std::collections::HashMap;

use futures::executor;
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command, time::Duration};
use typetag::serde;

#[typetag::serde(tag = "type")]
pub trait SecretBackend: core::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    fn retrieve(&mut self, secret_keys: Vec<String>) -> crate::Result<HashMap<String, String>>;
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ExecBackend {
    pub command: Vec<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout: u64,
}

const fn default_timeout_secs() -> u64 {
    5
}

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
    fn retrieve(&mut self, secret_keys: Vec<String>) -> crate::Result<HashMap<String, String>> {
        let mut output = executor::block_on(tokio::time::timeout(
            Duration::from_secs(self.timeout),
            query_backend(&self.command, new_query(secret_keys.clone())),
        ))??;
        let mut secrets = HashMap::new();
        for k in secret_keys.into_iter() {
            if let Some(secret) = output.get_mut(&k) {
                if let Some(e) = &secret.error {
                    return Err(format!("secret for key '{}' was not decrypted: {}", k, e).into());
                }
                if let Some(v) = secret.value.take() {
                    if v.len() == 0 {
                        return Err(format!("secret for key '{}' was empty", k).into());
                    }
                    secrets.insert(k, v);
                } else {
                    return Err(format!("secret for key '{}' was empty", k).into());
                }
            } else {
                return Err(format!("secret for key '{}' was not decrypted", k).into());
            }
        }
        Ok(secrets)
    }
}

async fn query_backend(
    cmd: &Vec<String>,
    query: ExecQuery,
) -> crate::Result<HashMap<String, ExecResponse>> {
    let command = &cmd[0];
    let mut command = Command::new(command);

    if cmd.len() > 1 {
        command.args(&cmd[1..]);
    };

    command.kill_on_drop(true);
    command.stderr(std::process::Stdio::piped());
    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());

    let mut child = command.spawn()?;
    let mut stdin = child.stdin.take().ok_or("unable to acquire stdin")?;

    let query = serde_json::to_vec(&query)?;
    stdin.write_all(&query).await?;
    drop(stdin);

    let output = child.wait_with_output().await?;
    let response = serde_json::from_slice::<HashMap<String, ExecResponse>>(&output.stdout)?;

    Ok(response)
}
