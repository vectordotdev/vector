//! Mock secret manager implementation used for testing the `secrets.exec` backend type.
//!
//! `tests/secrets.rs` executes this binary as the configured `command` of an `exec` secrets
//! backend, so it must implement the protocol documented in `src/secrets/mod.rs`. Its entire
//! stdout is parsed as a single JSON document, so nothing else may be written there.

use std::{
    collections::BTreeMap,
    io::{Read as _, Write as _},
    process::ExitCode,
};

use serde::Serialize;
use serde_json::{Map as JsonMap, Value};

/// Backends this mock pretends to support.
const SUPPORTED_BACKENDS: [&str; 1] = ["file.json"];

/// Backend config option required by the file backend.
const FILE_PATH_OPTION: &str = "file_path";

/// Error returned for secret keys that are not in [`FAKE_SECRETS`].
const MISSING_SECRET_ERROR: &str = "backend does not provide secret key";

/// Static fake secret cache. Keys that are not contained here are reported as an error.
const FAKE_SECRETS: [(&str, &str); 5] = [
    ("fake_secret_1", "123456"),
    ("fake_secret_2", "123457"),
    ("fake_secret_3", "123458"),
    ("fake_secret_4", "123459"),
    ("fake_secret_5", "123460"),
];

#[derive(Serialize)]
struct SecretResponse {
    value: Option<&'static str>,
    error: Option<&'static str>,
}

/// A validated request from Vector.
struct Request {
    secrets: Vec<String>,
    /// Backend type. Only present for version 1.1 requests.
    backend_type: Option<String>,
    /// Backend config. Only present for version 1.1 requests.
    config: Option<JsonMap<String, Value>>,
}

fn main() -> ExitCode {
    if let Err(error) = run() {
        let mut stderr = std::io::stderr();
        // Writing the error is best effort: the exit code reports the failure either way.
        _ = stderr.write_all(error.as_bytes());
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn run() -> Result<(), String> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|err| err.to_string())?;

    let request = parse_request(&input)?;
    let response = handle_request(&request)?;
    let response = serde_json::to_string(&response).map_err(|err| err.to_string())?;

    let mut stdout = std::io::stdout();
    stdout
        .write_all(response.as_bytes())
        .map_err(|err| err.to_string())?;
    stdout.flush().map_err(|err| err.to_string())
}

/// Validates a request by ensuring that it contains exactly the keys required by its version, and
/// that the values of those keys have the expected type.
fn parse_request(input: &str) -> Result<Request, String> {
    let request: JsonMap<String, Value> =
        serde_json::from_str(input).map_err(|err| err.to_string())?;

    let Some(version) = request.get("version") else {
        return Err("version key missing from request".to_owned());
    };
    let Some(version) = version.as_str() else {
        return Err(format!("Invalid version detected: {version}"));
    };

    match version {
        "1.0" => {
            ensure_keys(&request, version, &["secrets", "version"])?;
            Ok(Request {
                secrets: parse_secrets(&request)?,
                backend_type: None,
                config: None,
            })
        }
        "1.1" => {
            ensure_keys(&request, version, &["config", "secrets", "type", "version"])?;
            let backend_type = request
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| "key 'type' should be a str".to_owned())?;
            let config = request
                .get("config")
                .and_then(Value::as_object)
                .ok_or_else(|| "key 'config' should be a dict".to_owned())?;
            Ok(Request {
                secrets: parse_secrets(&request)?,
                backend_type: Some(backend_type.to_owned()),
                config: Some(config.clone()),
            })
        }
        other => Err(format!("Invalid version detected: {other}")),
    }
}

/// Verifies that the request contains exactly the keys expected for `version`.
fn ensure_keys(
    request: &JsonMap<String, Value>,
    version: &str,
    expected: &[&str],
) -> Result<(), String> {
    let mut actual: Vec<&str> = request.keys().map(String::as_str).collect();
    actual.sort_unstable();

    if actual != expected {
        let keys = actual.join(", ");
        return Err(format!(
            "Invalid required keys in {version} request: {keys}"
        ));
    }

    Ok(())
}

/// Extracts the requested secret keys, verifying that they are a list of strings.
fn parse_secrets(request: &JsonMap<String, Value>) -> Result<Vec<String>, String> {
    request
        .get("secrets")
        .and_then(Value::as_array)
        .ok_or_else(|| "key 'secrets' should be a list".to_owned())?
        .iter()
        .map(|secret| {
            secret
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "key 'secrets' should be a list of strings".to_owned())
        })
        .collect()
}

/// Looks every requested secret up in [`FAKE_SECRETS`], reporting an error for the keys that are
/// not contained there.
fn handle_request(request: &Request) -> Result<BTreeMap<String, SecretResponse>, String> {
    if let (Some(backend_type), Some(config)) = (&request.backend_type, &request.config) {
        if !SUPPORTED_BACKENDS.contains(&backend_type.as_str()) {
            return Err(format!("Requested backend: {backend_type} not supported"));
        }
        if !config.contains_key(FILE_PATH_OPTION) {
            return Err(format!(
                "File backend option {FILE_PATH_OPTION} must be supplied"
            ));
        }
    }

    Ok(request
        .secrets
        .iter()
        .map(|name| (name.clone(), lookup_secret(name)))
        .collect())
}

fn lookup_secret(name: &str) -> SecretResponse {
    match FAKE_SECRETS.iter().find(|(key, _)| *key == name) {
        Some((_, value)) => SecretResponse {
            value: Some(*value),
            error: None,
        },
        None => SecretResponse {
            value: None,
            error: Some(MISSING_SECRET_ERROR),
        },
    }
}
