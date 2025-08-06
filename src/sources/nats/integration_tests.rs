#[cfg(feature = "nats-integration-tests")]
#[cfg(test)]
mod integration_tests {
    #![allow(clippy::print_stdout)]
    use bytes::Bytes;
    use vector_lib::config::log_schema;

    use crate::{
        codecs::DecodingConfig,
        config::LogNamespace,
        nats::{
            NatsAuthConfig, NatsAuthCredentialsFile, NatsAuthNKey, NatsAuthToken,
            NatsAuthUserPassword,
        },
        serde::{default_decoding, default_framing_message_based},
        shutdown::ShutdownSignal,
        sources::nats::{
            config::{default_subject_key_field, BuildError, NatsSourceConfig},
            source::{create_subscription, run_nats_core},
        },
        test_util::{
            collect_n,
            components::{assert_source_compliance, SOURCE_TAGS},
            random_string,
        },
        tls::{TlsConfig, TlsEnableableConfig},
        SourceSender,
    };

    fn generate_source_config(url: &str, subject: &str) -> NatsSourceConfig {
        NatsSourceConfig {
            url: url.to_string(),
            subject: subject.to_string(),
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            subject_key_field: default_subject_key_field(),
            ..Default::default()
        }
    }

    async fn publish_and_check(conf: NatsSourceConfig) -> Result<(), BuildError> {
        let subject = conf.subject.clone();
        let (nc, sub) = create_subscription(&conf).await?;
        let nc_pub = nc.clone();
        let msg = "my message";

        let events = assert_source_compliance(&SOURCE_TAGS, async move {
            let (tx, rx) = SourceSender::new_test();
            let decoder = DecodingConfig::new(
                conf.framing.clone(),
                conf.decoding.clone(),
                LogNamespace::Legacy,
            )
            .build()
            .unwrap();
            tokio::spawn(run_nats_core(
                conf.clone(),
                nc,
                sub,
                decoder,
                LogNamespace::Legacy,
                ShutdownSignal::noop(),
                tx,
            ));
            nc_pub
                .publish(subject, Bytes::from_static(msg.as_bytes()))
                .await
                .unwrap();

            collect_n(rx, 1).await
        })
        .await;

        println!("Received event  {:?}", events[0].as_log());
        assert_eq!(
            events[0].as_log()[log_schema().message_key().unwrap().to_string()],
            msg.into()
        );
        Ok(())
    }

    #[tokio::test]
    async fn nats_no_auth() {
        let subject = format!("test-{}", random_string(10));
        let url =
            std::env::var("NATS_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = generate_source_config(&url, &subject);

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_userpass_auth_valid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_USERPASS_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let mut conf = generate_source_config(&url, &subject);
        conf.auth = Some(NatsAuthConfig::UserPassword {
            user_password: NatsAuthUserPassword {
                user: "natsuser".to_string(),
                password: "natspass".to_string().into(),
            },
        });

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_userpass_auth_invalid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_USERPASS_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let mut conf = generate_source_config(&url, &subject);
        conf.auth = Some(NatsAuthConfig::UserPassword {
            user_password: NatsAuthUserPassword {
                user: "natsuser".to_string(),
                password: "wrongpass".to_string().into(),
            },
        });

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_token_auth_valid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TOKEN_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let mut conf = generate_source_config(&url, &subject);
        conf.auth = Some(NatsAuthConfig::Token {
            token: NatsAuthToken {
                value: "secret".to_string().into(),
            },
        });

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_token_auth_invalid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TOKEN_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let mut conf = generate_source_config(&url, &subject);
        conf.auth = Some(NatsAuthConfig::Token {
            token: NatsAuthToken {
                value: "wrongsecret".to_string().into(),
            },
        });

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_nkey_auth_valid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_NKEY_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let mut conf = generate_source_config(&url, &subject);
        conf.auth = Some(NatsAuthConfig::Nkey {
            nkey: NatsAuthNKey {
                nkey: "UD345ZYSUJQD7PNCTWQPINYSO3VH4JBSADBSYUZOBT666DRASFRAWAWT".into(),
                seed: "SUANIRXEZUROTXNFN3TJYMT27K7ZZVMD46FRIHF6KXKS4KGNVBS57YAFGY".into(),
            },
        });

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_nkey_auth_invalid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_NKEY_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let mut conf = generate_source_config(&url, &subject);
        conf.auth = Some(NatsAuthConfig::Nkey {
            nkey: NatsAuthNKey {
                nkey: "UAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                seed: "SBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into(),
            },
        });

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Config, got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_tls_valid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TLS_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let mut conf = generate_source_config(&url, &subject);
        conf.tls = Some(TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                ca_file: Some("tests/data/nats/rootCA.pem".into()),
                ..Default::default()
            },
        });

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_tls_invalid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TLS_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = generate_source_config(&url, &subject);

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_tls_client_cert_valid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TLS_CLIENT_CERT_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let mut conf = generate_source_config(&url, &subject);
        conf.tls = Some(TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                ca_file: Some("tests/data/nats/rootCA.pem".into()),
                crt_file: Some("tests/data/nats/nats-client.pem".into()),
                key_file: Some("tests/data/nats/nats-client.key".into()),
                ..Default::default()
            },
        });

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_tls_client_cert_invalid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TLS_CLIENT_CERT_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let mut conf = generate_source_config(&url, &subject);
        conf.tls = Some(TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                ca_file: Some("tests/data/nats/rootCA.pem".into()),
                ..Default::default()
            },
        });

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_tls_jwt_auth_valid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_JWT_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let mut conf = generate_source_config(&url, &subject);
        conf.tls = Some(TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                ca_file: Some("tests/data/nats/rootCA.pem".into()),
                ..Default::default()
            },
        });
        conf.auth = Some(NatsAuthConfig::CredentialsFile {
            credentials_file: NatsAuthCredentialsFile {
                path: "tests/data/nats/nats.creds".into(),
            },
        });

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_tls_jwt_auth_invalid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_JWT_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let mut conf = generate_source_config(&url, &subject);
        conf.tls = Some(TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                ca_file: Some("tests/data/nats/rootCA.pem".into()),
                ..Default::default()
            },
        });
        conf.auth = Some(NatsAuthConfig::CredentialsFile {
            credentials_file: NatsAuthCredentialsFile {
                path: "tests/data/nats/nats-bad.creds".into(),
            },
        });

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_multiple_urls_valid() {
        let subject = format!("test-{}", random_string(10));
        let url = "nats://nats:4222,nats://demo.nats.io:4222";

        let conf = generate_source_config(url, &subject);

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed for multiple URLs, expected Ok(()), got: {r:?}"
        );
    }

    #[tokio::test]
    async fn nats_multiple_urls_invalid() {
        let subject = format!("test-{}", random_string(10));
        let url = "http://invalid-url,nats://:invalid@localhost:4222";

        let conf = generate_source_config(url, &subject);

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed for bad URLs, expected BuildError::Connect, got: {r:?}"
        );
    }
}
