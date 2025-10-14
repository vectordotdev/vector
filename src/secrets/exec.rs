use std::collections::{HashMap, HashSet};

use bytes::BytesMut;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command, time};
use tokio_util::codec;
use vector_lib::configurable::{component::GenerateConfig, configurable_component};
use vrl::value::Value;

use crate::{config::SecretBackend, signal};

/// Configuration for the command that will be `exec`ed
#[configurable_component(secrets("exec"))]
#[configurable(metadata(docs::enum_tag_description = "The protocol version."))]
#[derive(Clone, Debug)]
#[serde(rename_all = "snake_case", tag = "version")]
pub enum ExecVersion {
    /// Expect the command to fetch the configuration options itself.
    V1,

    /// Configuration options to the command are to be curried upon each request.
    V1_1 {
        /// The name of the backend. This is `type` field in the backend request.
        backend_type: String,
        /// The configuration to pass to the secrets executable. This is the `config` field in the
        /// backend request. Refer to the documentation of your `backend_type `to see which options
        /// are required to be set.
        backend_config: Value,
    },
}

impl ExecVersion {
    fn new_query(&self, secrets: HashSet<String>) -> ExecQuery {
        match &self {
            ExecVersion::V1 => ExecQuery {
                version: "1.0".to_string(),
                secrets,
                r#type: None,
                config: None,
            },
            ExecVersion::V1_1 {
                backend_type,
                backend_config,
                ..
            } => ExecQuery {
                version: "1.1".to_string(),
                secrets,
                r#type: Some(backend_type.clone()),
                config: Some(backend_config.clone()),
            },
        }
    }
}

impl GenerateConfig for ExecVersion {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(ExecVersion::V1).unwrap()
    }
}

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

    /// Settings for the protocol between Vector and the secrets executable.
    #[serde(default = "default_protocol_version")]
    pub protocol: ExecVersion,
}

impl GenerateConfig for ExecBackend {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(ExecBackend {
            command: vec![String::from("/path/to/script")],
            timeout: 5,
            protocol: ExecVersion::V1,
        })
        .unwrap()
    }
}

const fn default_timeout_secs() -> u64 {
    5
}

const fn default_protocol_version() -> ExecVersion {
    ExecVersion::V1
}

#[derive(Clone, Debug, Serialize)]
struct ExecQuery {
    // Fields in all versions starting from v1
    version: String,
    secrets: HashSet<String>,
    // Fields added in v1.1
    #[serde(skip_serializing_if = "Option::is_none")]
    r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ExecResponse {
    value: Option<String>,
    error: Option<String>,
}

impl SecretBackend for ExecBackend {
    async fn retrieve(
        &mut self,
        secret_keys: HashSet<String>,
        signal_rx: &mut signal::SignalRx,
    ) -> crate::Result<HashMap<String, String>> {
        let mut output = query_backend(
            &self.command,
            self.protocol.new_query(secret_keys.clone()),
            self.timeout,
            signal_rx,
        )
        .await?;
        let mut secrets = HashMap::new();
        for k in secret_keys.into_iter() {
            if let Some(secret) = output.get_mut(&k) {
                if let Some(e) = &secret.error {
                    return Err(format!("secret for key '{k}' was not retrieved: {e}").into());
                }
                if let Some(v) = secret.value.take() {
                    if v.is_empty() {
                        return Err(format!("secret for key '{k}' was empty").into());
                    }
                    secrets.insert(k.to_string(), v);
                } else {
                    return Err(format!("secret for key '{k}' was empty").into());
                }
            } else {
                return Err(format!("secret for key '{k}' was not retrieved").into());
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
        .ok_or("unable to acquire stderr")?;
    let mut stdout_stream = child
        .stdout
        .map(|s| codec::FramedRead::new(s, codec::BytesCodec::new()))
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
                    Some(Err(e)) => return Err(format!("Error while reading from an exec backend stdout: {e}.").into()),
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

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
    };

    use rstest::rstest;
    use tokio::sync::broadcast;
    use vrl::value;

    use crate::{
        config::SecretBackend,
        secrets::exec::{ExecBackend, ExecVersion},
    };

    fn make_test_backend(protocol: ExecVersion) -> ExecBackend {
        let command_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/behavior/secrets/mock_secrets_exec.py");
        ExecBackend {
            command: ["python", command_path.to_str().unwrap()]
                .map(String::from)
                .to_vec(),
            timeout: 5,
            protocol,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[rstest(
        protocol,
        case(ExecVersion::V1),
        case(ExecVersion::V1_1 {
            backend_type: "file.json".to_string(),
            backend_config: value!({"file_path": "/abc.json"}),
        })
    )]
    async fn test_exec_backend(protocol: ExecVersion) {
        let mut backend = make_test_backend(protocol);
        let (_tx, mut rx) = broadcast::channel(1);
        // These fake secrets are statically contained in mock_secrets_exec.py
        let fake_secret_values: HashMap<String, String> = [
            ("fake_secret_1", "123456"),
            ("fake_secret_2", "123457"),
            ("fake_secret_3", "123458"),
            ("fake_secret_4", "123459"),
            ("fake_secret_5", "123460"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        // Calling the mock_secrets_exec.py program with the expected secret keys should provide
        // the values expected above in `fake_secret_values`
        let fetched_keys = backend
            .retrieve(fake_secret_values.keys().cloned().collect(), &mut rx)
            .await
            .unwrap();
        // Assert response is as expected
        assert_eq!(fetched_keys.len(), 5);
        for (fake_secret_key, fake_secret_value) in fake_secret_values {
            assert_eq!(fetched_keys.get(&fake_secret_key), Some(&fake_secret_value));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_exec_backend_missing_secrets() {
        let mut backend = make_test_backend(ExecVersion::V1);
        let (_tx, mut rx) = broadcast::channel(1);
        let query_secrets: HashSet<String> =
            ["fake_secret_900"].into_iter().map(String::from).collect();
        let fetched_keys = backend.retrieve(query_secrets.clone(), &mut rx).await;
        assert_eq!(
            format!("{}", fetched_keys.unwrap_err()),
            "secret for key 'fake_secret_900' was not retrieved: backend does not provide secret key"
        );
    }
}
