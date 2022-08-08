use std::{collections::HashMap, io::Read};

use bytes::BytesMut;
use futures::{executor, StreamExt};
use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command, time};
use tokio_util::codec;
use toml::value::Table;
use typetag::serde;

use super::{loader, prepare_input};
use crate::{
    config::{
        loading::{deserialize_table, ComponentHint, Process},
        ComponentKey,
    },
    signal,
};

// The following regex aims to extract a pair of strings, the first being the secret backend name
// and the second being the secret key. Here are some matching & non-matching examples:
// - "SECRET[backend.secret_name]" will match and capture "backend" and "secret_name"
// - "SECRET[backend.secret.name]" will match and catpure "backend" and "secret.name"
// - "SECRET[backend..secret.name]" will match and catpure "backend" and ".secret.name"
// - "SECRET[secret_name]" will not match
// - "SECRET[.secret.name]" wil not match
static COLLECTOR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"SECRET\[([[:word:]]+)\.([[:word:].]+)\]").unwrap());

#[typetag::serde(tag = "type")]
pub trait SecretBackend: core::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    fn retrieve(
        &mut self,
        secret_keys: Vec<String>,
        signal_rx: &mut signal::SignalRx,
    ) -> crate::Result<HashMap<String, String>>;
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct SecretBackendLoader {
    backends: IndexMap<ComponentKey, Box<dyn SecretBackend>>,
    pub(crate) secret_keys: HashMap<String, Vec<String>>,
}

impl SecretBackendLoader {
    pub(crate) fn new() -> Self {
        Self {
            backends: IndexMap::new(),
            secret_keys: HashMap::new(),
        }
    }

    pub(crate) fn retrieve(
        &mut self,
        signal_rx: &mut signal::SignalRx,
    ) -> Result<HashMap<String, String>, String> {
        let secrets = self.secret_keys.iter().flat_map(|(backend_name, keys)| {
            match self.backends.get_mut(&ComponentKey::from(backend_name.clone())) {
                None => {
                    vec![Err(format!("Backend \"{}\" is required for secret retrieval but was not found in config.", backend_name))]
                },
                Some(backend) => {
                    debug!(message = "Retrieving secret from a backend.", backend = ?backend_name);
                    match backend.retrieve(keys.to_vec(), signal_rx) {
                        Err(e) => {
                            vec![Err(format!("Error while retrieving secret from backend \"{}\": {}.", backend_name, e))]
                        },
                        Ok(s) => {
                            s.into_iter().map(|(k, v)| {
                                trace!(message = "Successfully retrieved a secret.", backend = ?backend_name, secret_key = ?k);
                                Ok((format!("{}.{}", backend_name, k), v))
                            }).collect::<Vec<Result<(String, String), String>>>()
                        }
                    }
                },
            }
        }).collect::<Result<HashMap<String,String>,String>>()?;
        Ok(secrets)
    }

    pub(crate) fn has_secrets_to_retrieve(&self) -> bool {
        !self.secret_keys.is_empty()
    }
}

impl Process for SecretBackendLoader {
    fn prepare<R: Read>(&mut self, input: R) -> Result<(String, Vec<String>), Vec<String>> {
        let (config_string, warnings) = prepare_input(input)?;
        // Collect secret placeholders just after env var processing
        collect_secret_keys(&config_string, &mut self.secret_keys);
        Ok((config_string, warnings))
    }

    fn merge(&mut self, table: Table, _: Option<ComponentHint>) -> Result<(), Vec<String>> {
        if table.contains_key("secret") {
            let additional = deserialize_table::<SecretBackends>(table)?;
            self.backends.extend(additional.secret);
        }
        Ok(())
    }
}

impl loader::Loader<SecretBackendLoader> for SecretBackendLoader {
    /// Returns the resulting `SecretBackendLoader`.
    fn take(self) -> SecretBackendLoader {
        self
    }
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub(crate) struct SecretBackends {
    #[serde(default)]
    pub(crate) secret: IndexMap<ComponentKey, Box<dyn SecretBackend>>,
}

pub fn interpolate(input: &str, secrets: &HashMap<String, String>) -> Result<String, Vec<String>> {
    let mut errors = Vec::<String>::new();
    let output = COLLECTOR
        .replace_all(input, |caps: &Captures<'_>| {
            caps.get(1)
                .and_then(|b| caps.get(2).map(|k| (b, k)))
                .and_then(|(b, k)| secrets.get(&format!("{}.{}", b.as_str(), k.as_str())))
                .cloned()
                .unwrap_or_else(|| {
                    errors.push(format!(
                        "Unable to find secret replacement for {}.",
                        caps.get(0).unwrap().as_str()
                    ));
                    "".to_string()
                })
        })
        .into_owned();
    if errors.is_empty() {
        Ok(output)
    } else {
        Err(errors)
    }
}

fn collect_secret_keys(input: &str, keys: &mut HashMap<String, Vec<String>>) {
    COLLECTOR.captures_iter(input).for_each(|cap| {
        if let (Some(backend), Some(key)) = (cap.get(1), cap.get(2)) {
            if let Some(keys) = keys.get_mut(backend.as_str()) {
                keys.push(key.as_str().to_string());
            } else {
                keys.insert(backend.as_str().to_string(), vec![key.as_str().to_string()]);
            }
        }
    });
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
    fn retrieve(
        &mut self,
        secret_keys: Vec<String>,
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
                    secrets.insert(k, v);
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
            Ok(signal::SignalTo::Shutdown | signal::SignalTo::Quit) = signal_rx.recv() => {
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

#[derive(Deserialize, Serialize, Debug, Clone)]
struct TestBackend {
    pub replacement: String,
}

#[typetag::serde(name = "test")]
impl SecretBackend for TestBackend {
    fn retrieve(
        &mut self,
        secret_keys: Vec<String>,
        _: &mut signal::SignalRx,
    ) -> crate::Result<HashMap<String, String>> {
        Ok(secret_keys
            .into_iter()
            .map(|k| (k, self.replacement.clone()))
            .collect())
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use indoc::indoc;

    use super::{collect_secret_keys, interpolate};

    #[test]
    fn replacement() {
        let secrets: HashMap<String, String> = vec![
            ("a.secret.key".into(), "value".into()),
            ("a...key".into(), "a...value".into()),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            Ok("value".into()),
            interpolate("SECRET[a.secret.key]", &secrets)
        );
        assert_eq!(
            Ok("value value".into()),
            interpolate("SECRET[a.secret.key] SECRET[a.secret.key]", &secrets)
        );

        assert_eq!(
            Ok("xxxvalueyyy".into()),
            interpolate("xxxSECRET[a.secret.key]yyy", &secrets)
        );
        assert_eq!(
            Ok("a...value".into()),
            interpolate("SECRET[a...key]", &secrets)
        );
        assert_eq!(
            Ok("xxxSECRET[non_matching_syntax]yyy".into()),
            interpolate("xxxSECRET[non_matching_syntax]yyy", &secrets)
        );
        assert_eq!(
            Err(vec![
                "Unable to find secret replacement for SECRET[a.non.existing.key].".into()
            ]),
            interpolate("xxxSECRET[a.non.existing.key]yyy", &secrets)
        );
    }

    #[test]
    fn collection() {
        let mut keys = HashMap::<String, Vec<String>>::new();
        collect_secret_keys(
            indoc! {r#"
            SECRET[first_backend.secret_key]
            SECRET[first_backend.another_secret_key]
            SECRET[second_backend.secret_key]
            SECRET[second_backend.secret.key]
            SECRET[first_backend.a_third.secret_key]
            SECRET[first_backend...an_extra_secret_key]
            SECRET[non_matching_syntax]
            SECRET[.non.matching.syntax]
        "#},
            &mut keys,
        );
        assert_eq!(keys.len(), 2);
        assert!(keys.contains_key("first_backend"));
        assert!(keys.contains_key("second_backend"));

        let first_backend_keys = keys.get("first_backend").unwrap();
        assert_eq!(first_backend_keys.len(), 4);
        assert!(first_backend_keys.contains(&"secret_key".into()));
        assert!(first_backend_keys.contains(&"another_secret_key".into()));
        assert!(first_backend_keys.contains(&"a_third.secret_key".into()));
        assert!(first_backend_keys.contains(&"..an_extra_secret_key".into()));

        let second_backend_keys = keys.get("second_backend").unwrap();
        assert_eq!(second_backend_keys.len(), 2);
        assert!(second_backend_keys.contains(&"secret_key".into()));
        assert!(second_backend_keys.contains(&"secret.key".into()));
    }
}
