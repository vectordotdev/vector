use std::collections::HashMap;

use futures::executor;
use indexmap::IndexMap;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command, time::Duration};
use typetag::serde;

use super::{format, ComponentKey, Format};

#[typetag::serde(tag = "type")]
pub trait SecretBackend: core::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    fn retrieve(&mut self, secret_keys: Vec<String>) -> crate::Result<HashMap<String, String>>;
}

#[derive(Deserialize, Serialize, Debug, Default)]
struct SecretBackends {
    #[serde(default)]
    secret: IndexMap<ComponentKey, Box<dyn SecretBackend>>,
}

pub fn interpolate(input: &str, format: Format) -> (String, Vec<String>) {
    let keys = collect_secret_keys(input);
    if keys.is_empty() {
        debug!("No secret placeholder found, skipping secret resolution.");
        return (input.to_owned(), Vec::new());
    }
    let backends = format::deserialize::<SecretBackends>(input, format);
    match backends {
        Err(e) => (input.to_owned(), e),
        Ok(backends) => {
            let (secrets, warnings) = retrieve_secrets(keys, backends);
            (do_replace(input, secrets), warnings)
        }
    }
}

fn retrieve_secrets(
    keys: HashMap<String, Vec<String>>,
    mut backends: SecretBackends,
) -> (HashMap<String, String>, Vec<String>) {
    let mut warnings = Vec::<String>::new();
    let secrets = keys.into_iter().flat_map(|(backend_name, keys)| {
        match backends.secret.get_mut(&ComponentKey::from(backend_name.clone())) {
            None => {
                warnings.push(format!("Backend \"{}\" is required for secret retrieval but was not found in config.", backend_name));
                vec![]
            },
            Some(backend) => {
                match backend.retrieve(keys) {
                    Err(e) => {
                        warnings.push(e.to_string());
                        vec![]
                    },
                    Ok(s) => {
                        s.into_iter().map(|(k, v)| {
                            (format!("{}.{}", backend_name, k), v)
                        }).collect::<Vec<(String, String)>>()
                    }
                }
            },
        }
    }).collect::<HashMap<String,String>>();
    (secrets, warnings)
}

fn do_replace(input: &str, secrets: HashMap<String, String>) -> String {
    let re = Regex::new(r"SECRET\[([[:word:]]+\.[[:word:].]+)\]").unwrap();
    re.replace_all(input, |caps: &Captures<'_>| {
        caps.get(1)
            .map(|k| secrets.get(k.as_str()))
            .flatten()
            .cloned()
            .unwrap_or_else(|| "".to_string())
    })
    .into_owned()
}

fn collect_secret_keys(input: &str) -> HashMap<String, Vec<String>> {
    let re = Regex::new(r"SECRET\[([[:word:]]+)\.([[:word:].]+)\]").unwrap();
    let mut keys: HashMap<String, Vec<String>> = HashMap::new();
    re.captures_iter(input).for_each(|cap| {
        if let (Some(backend), Some(key)) = (cap.get(1), cap.get(2)) {
            if let Some(keys) = keys.get_mut(backend.as_str()) {
                keys.push(key.as_str().to_string());
            } else {
                keys.insert(backend.as_str().to_string(), vec![key.as_str().to_string()]);
            }
        }
    });
    keys
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
    fn retrieve(&mut self, secret_keys: Vec<String>) -> crate::Result<HashMap<String, String>> {
        let mut output = executor::block_on(tokio::time::timeout(
            Duration::from_secs(self.timeout),
            query_backend(&self.command, new_query(secret_keys.clone())),
        ))??;
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

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use indoc::indoc;

    use super::{collect_secret_keys, do_replace};

    #[test]
    fn replacement() {
        let secrets: HashMap<String, String> = vec![
            ("a.secret.key".into(), "value".into()),
            ("a...key".into(), "a...value".into()),
        ]
        .into_iter()
        .collect();

        assert_eq!("value", do_replace("SECRET[a.secret.key]", secrets.clone()));
        assert_eq!(
            "xxxvalueyyy",
            do_replace("xxxSECRET[a.secret.key]yyy", secrets.clone())
        );
        assert_eq!("a...value", do_replace("SECRET[a...key]", secrets.clone()));
        assert_eq!(
            "xxxSECRET[non_matching_syntax]yyy",
            do_replace("xxxSECRET[non_matching_syntax]yyy", secrets.clone())
        );
        assert_eq!(
            "xxxyyy",
            do_replace("xxxSECRET[a.non.existing.key]yyy", secrets)
        );
    }

    #[test]
    fn collection() {
        let keys = collect_secret_keys(indoc! {r#"
            SECRET[first_backend.secret_key]
            SECRET[first_backend.another_secret_key]
            SECRET[second_backend.secret_key]
            SECRET[second_backend.secret.key]
            SECRET[first_backend.a_third.secret_key]
            SECRET[non_matching_syntax]
        "#});
        assert_eq!(keys.len(), 2);
        assert!(keys.contains_key("first_backend"));
        assert!(keys.contains_key("second_backend"));
    }
}
