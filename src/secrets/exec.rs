use std::collections::{HashMap, HashSet};

use bytes::BytesMut;
use futures::executor;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command, time};
use tokio_util::codec;
use vector_lib::configurable::{component::GenerateConfig, configurable_component};

use crate::{config::SecretBackend, signal};

/// Configuration for the `exec` secrets backend.
#[configurable_component(secrets("exec"))]
#[derive(Clone, Debug)]
pub struct ExecBackend {
    /// Command arguments to execute.
    ///
    /// The path to the script or binary must be the first argument.
    pub command: Vec<String>,

    /// The timeout, in seconds, to wait for the command to complete.
    #[serde(default = "default_timeout_secs")]
    pub timeout: u64,
}

impl GenerateConfig for ExecBackend {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(ExecBackend {
            command: vec![String::from("/path/to/script")],
            timeout: 5,
        })
        .unwrap()
    }
}

const fn default_timeout_secs() -> u64 {
    5
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ExecQuery {
    version: String,
    secrets: HashSet<String>,
}

fn new_query(secrets: HashSet<String>) -> ExecQuery {
    ExecQuery {
        version: "1.0".to_string(),
        secrets,
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ExecResponse {
    value: Option<String>,
    error: Option<String>,
}

impl SecretBackend for ExecBackend {
    fn retrieve(
        &mut self,
        secret_keys: HashSet<String>,
        signal_rx: &mut signal::SignalRx,
    ) -> crate::Result<HashMap<String, String>> {
        let mut output = executor::block_on(async {
            query_backend(
                &self.command,
                new_query(secret_keys.clone()),
                self.timeout,
                signal_rx,
            )
            .await
        })?;
        let mut secrets = HashMap::new();
        for k in secret_keys.into_iter() {
            if let Some(secret) = output.get_mut(&k) {
                if let Some(e) = &secret.error {
                    return Err(format!("secret for key '{}' was not retrieved: {}", k, e).into());
                }
                if let Some(v) = secret.value.take() {
                    if v.is_empty() {
                        return Err(format!("secret for key '{}' was empty", k).into());
                    }
                    secrets.insert(k.to_string(), v);
                } else {
                    return Err(format!("secret for key '{}' was empty", k).into());
                }
            } else {
                return Err(format!("secret for key '{}' was not retrieved", k).into());
            }
        }
        Ok(secrets)
    }
}

async fn query_backend(
    cmd: &[String],
    query: ExecQuery,
    timeout: u64,
    signal_rx: &mut signal::SignalRx,
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
    let mut stderr_stream = child
        .stderr
        .map(|s| codec::FramedRead::new(s, codec::LinesCodec::new()))
        .take()
        .ok_or("unable to acquire stderr")?;
    let mut stdout_stream = child
        .stdout
        .map(|s| codec::FramedRead::new(s, codec::BytesCodec::new()))
        .take()
        .ok_or("unable to acquire stdout")?;

    let query = serde_json::to_vec(&query)?;
    tokio::spawn(async move { stdin.write_all(&query).await });

    let timeout = time::sleep(time::Duration::from_secs(timeout));
    tokio::pin!(timeout);
    let mut output = BytesMut::new();
    loop {
        tokio::select! {
            biased;
            Ok(signal::SignalTo::Shutdown(_) | signal::SignalTo::Quit) = signal_rx.recv() => {
                drop(command);
                return Err("Secret retrieval was interrupted.".into());
            }
            Some(stderr) = stderr_stream.next() => {
                match stderr {
                    Ok(l) => warn!("An exec backend generated message on stderr: {}.", l),
                    Err(e) => warn!("Error while reading from an exec backend stderr: {}.", e),
                }
            }
            stdout = stdout_stream.next() => {
                match stdout {
                    None => break,
                    Some(Ok(b)) => output.extend(b),
                    Some(Err(e)) => return Err(format!("Error while reading from an exec backend stdout: {}.", e).into()),
                }
            }
            _ = &mut timeout => {
                drop(command);
                return Err("Command timed-out".into());
            }
        }
    }

    let response = serde_json::from_slice::<HashMap<String, ExecResponse>>(&output)?;
    Ok(response)
}
