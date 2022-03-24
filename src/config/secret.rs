use std::collections::HashMap;

use futures::executor;
use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command, time::Duration};
use typetag::serde;

use super::{format, ComponentKey, Format};

#[typetag::serde(tag = "type")]
pub trait SecretBackend: core::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    fn retrieve(&mut self, secret_keys: Vec<String>) -> crate::Result<HashMap<String, String>>;
}

#[derive(Deserialize, Serialize, Debug, Default)]
struct SecretBackendConfigBuilder {
    #[serde(default)]
    secret: IndexMap<ComponentKey, Box<dyn SecretBackend>>,
}

pub fn interpolate(input: &str, format: Format) -> (String, Vec<String>) {
    let keys = collect_keys(input);
    if keys.is_empty() {
        debug!("No secret placeholder found, skipping secret resolution.");
        return (input.to_owned(), Vec::new());
    }
    debug!("{:#?}", keys);
    let secret_backends = format::deserialize::<SecretBackendConfigBuilder>(input, format);
    match secret_backends {
        Err(e) => {
            return (input.to_owned(), e);
        }
        Ok(s) => {
            return (input.to_owned(), Vec::new());
        }
    }
}

fn collect_keys(input: &str) -> HashMap<&str, Vec<&str>> {
    let re = Regex::new(r"SECRET\[([[:word:]]+)\.([[:word:].]+)\]").unwrap();
    let mut keys: HashMap<&str, Vec<&str>> = HashMap::new();
    re.captures_iter(input).for_each(|cap| {
        if let (Some(backend), Some(key)) = (cap.get(1), cap.get(2)) {
            if let Some(keys) = keys.get_mut(backend.as_str()) {
                keys.push(key.as_str());
            } else {
                keys.insert(backend.as_str(), vec![key.as_str()]);
            }
        }
    });
    keys
}

///////////

#[derive(Deserialize, Serialize, Debug, Clone)]
struct ExecBackend {
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
