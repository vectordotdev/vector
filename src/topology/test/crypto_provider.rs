use std::sync::Arc;

use crate::{
    config::{ConfigBuilder, ConfigDiff},
    test_util::trace_init,
    topology::builder::TopologyPiecesBuilder,
};

#[test]
fn initializes_crypto_provider_without_cli() {
    // Rustls's process-wide provider cannot be reset between test cases.
    let Ok(mode) = std::env::var("__VECTOR_TOPOLOGY_CRYPTO_PROVIDER_TEST") else {
        for mode in ["unset", "existing"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .env("__VECTOR_TOPOLOGY_CRYPTO_PROVIDER_TEST", mode)
                .args([
                    "--exact",
                    "topology::test::crypto_provider::initializes_crypto_provider_without_cli",
                    "--nocapture",
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode} provider subprocess failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
        return;
    };

    assert!(rustls::crypto::CryptoProvider::get_default().is_none());
    let existing = if mode == "existing" {
        let mut provider = rustls::crypto::ring::default_provider();
        provider.cipher_suites.truncate(1);
        provider.install_default().unwrap();
        Some(Arc::clone(
            rustls::crypto::CryptoProvider::get_default().unwrap(),
        ))
    } else {
        None
    };

    trace_init();
    let config: ConfigBuilder = serde_yaml::from_str(indoc::indoc! {r"
        sources:
          input:
            type: test_basic
        sinks:
          output:
            type: greptimedb_metrics
            inputs: [input]
            endpoint: localhost:4001
            tls:
              ca_file: tests/data/ca/certs/ca.cert.pem
              crt_file: tests/data/ca/intermediate_client/certs/localhost-chain.cert.pem
              key_file: tests/data/ca/intermediate_client/private/localhost.key.pem
    "})
    .unwrap();
    let config = config.build().unwrap();
    let diff = ConfigDiff::initial(&config);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // Build TLS clients, but do not run healthchecks or contact a server.
    let _pieces = runtime
        .block_on(TopologyPiecesBuilder::new(&config, &diff).build())
        .expect("TLS topology should build without CLI initialization");
    let provider = rustls::crypto::CryptoProvider::get_default()
        .expect("topology construction should install a provider");
    if let Some(existing) = existing {
        assert!(Arc::ptr_eq(&existing, provider));
    }
}
