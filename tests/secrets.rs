//! Tests for the `secrets.exec` backend, which executes an external command to retrieve secrets.
//!
//! This lives in an integration test target because it needs the `mock-secrets-exec` binary, which
//! `cargo` builds alongside integration test targets and exposes via `CARGO_BIN_EXE_*`. The mock
//! implementation is `tests/behavior/secrets/mock_secrets_exec.rs`.

use std::collections::{HashMap, HashSet};

use rstest::rstest;
use tokio::sync::broadcast;
use vector::{
    config::SecretBackend,
    secrets::exec::{ExecBackend, ExecVersion},
};
use vrl::value;

fn make_test_backend(protocol: ExecVersion) -> ExecBackend {
    ExecBackend {
        command: vec![env!("CARGO_BIN_EXE_mock-secrets-exec").to_owned()],
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
    // These fake secrets are statically contained in mock_secrets_exec.rs
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
    // Calling the mock_secrets_exec binary with the expected secret keys should provide the values
    // expected above in `fake_secret_values`
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
