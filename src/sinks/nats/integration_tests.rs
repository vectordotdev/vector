use std::time::Duration;
use vector_lib::codecs::TextSerializerConfig;

use super::{config::NatsSinkConfig, sink::NatsSink, NatsError};
use crate::{
    nats::{
        NatsAuthConfig, NatsAuthCredentialsFile, NatsAuthNKey, NatsAuthToken, NatsAuthUserPassword,
    },
    sinks::prelude::*,
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        random_lines_with_stream, random_string, trace_init,
    },
    tls::TlsEnableableConfig,
};

async fn publish_and_check(conf: NatsSinkConfig) -> Result<(), NatsError> {
    // Publish `N` messages to NATS.
    //
    // Verify with a separate subscriber that the messages were
    // successfully published.

    // Create Sink
    let sink = NatsSink::new(conf.clone()).await?;
    let sink = VectorSink::from_event_streamsink(sink);

    // Establish the consumer subscription.
    let subject = conf.subject.clone();
    let consumer = conf
        .clone()
        .connect()
        .await
        .expect("failed to connect with test consumer");
    let mut sub = consumer
        .subscribe(subject.to_string())
        .await
        .expect("failed to subscribe with test consumer");
    consumer
        .flush()
        .await
        .expect("failed to flush with the test consumer");

    // Publish events.
    let num_events = 10;
    let (input, events) = random_lines_with_stream(100, num_events, None);

    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;

    // Unsubscribe from the channel.
    tokio::time::sleep(Duration::from_secs(3)).await;
    sub.unsubscribe().await.unwrap();

    let mut output: Vec<String> = Vec::new();
    while let Some(msg) = sub.next().await {
        output.push(String::from_utf8_lossy(&msg.payload).to_string())
    }

    assert_eq!(output.len(), input.len());
    assert_eq!(output, input);

    Ok(())
}

#[tokio::test]
async fn nats_no_auth() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url =
        std::env::var("NATS_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: None,
        auth: None,
        request: Default::default(),
        jetstream: false,
    };

    let r = publish_and_check(conf).await;
    assert!(
        r.is_ok(),
        "publish_and_check failed, expected Ok(()), got: {:?}",
        r
    );
}

#[tokio::test]
async fn nats_userpass_auth_valid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_USERPASS_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: None,
        auth: Some(NatsAuthConfig::UserPassword {
            user_password: NatsAuthUserPassword {
                user: "natsuser".to_string(),
                password: "natspass".to_string().into(),
            },
        }),
        request: Default::default(),
        jetstream: false,
    };

    publish_and_check(conf)
        .await
        .expect("publish_and_check failed");
}

#[tokio::test]
async fn nats_userpass_auth_invalid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_USERPASS_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: None,
        auth: Some(NatsAuthConfig::UserPassword {
            user_password: NatsAuthUserPassword {
                user: "natsuser".to_string(),
                password: "wrongpass".to_string().into(),
            },
        }),
        request: Default::default(),
        jetstream: false,
    };

    let r = publish_and_check(conf).await;
    assert!(
        matches!(r, Err(NatsError::Connect { .. })),
        "publish_and_check failed, expected NatsError::Connect, got: {:?}",
        r
    );
}

#[tokio::test]
async fn nats_token_auth_valid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_TOKEN_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: None,
        auth: Some(NatsAuthConfig::Token {
            token: NatsAuthToken {
                value: "secret".to_string().into(),
            },
        }),
        request: Default::default(),
        jetstream: false,
    };

    let r = publish_and_check(conf).await;
    assert!(
        r.is_ok(),
        "publish_and_check failed, expected Ok(()), got: {:?}",
        r
    );
}

#[tokio::test]
async fn nats_token_auth_invalid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_TOKEN_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: None,
        auth: Some(NatsAuthConfig::Token {
            token: NatsAuthToken {
                value: "wrongsecret".to_string().into(),
            },
        }),
        request: Default::default(),
        jetstream: false,
    };

    let r = publish_and_check(conf).await;
    assert!(
        matches!(r, Err(NatsError::Connect { .. })),
        "publish_and_check failed, expected NatsError::Connect, got: {:?}",
        r
    );
}

#[tokio::test]
async fn nats_nkey_auth_valid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_NKEY_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: None,
        auth: Some(NatsAuthConfig::Nkey {
            nkey: NatsAuthNKey {
                nkey: "UD345ZYSUJQD7PNCTWQPINYSO3VH4JBSADBSYUZOBT666DRASFRAWAWT".into(),
                seed: "SUANIRXEZUROTXNFN3TJYMT27K7ZZVMD46FRIHF6KXKS4KGNVBS57YAFGY".into(),
            },
        }),
        request: Default::default(),
        jetstream: false,
    };

    let r = publish_and_check(conf).await;
    assert!(
        r.is_ok(),
        "publish_and_check failed, expected Ok(()), got: {:?}",
        r
    );
}

#[tokio::test]
async fn nats_nkey_auth_invalid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_NKEY_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: None,
        auth: Some(NatsAuthConfig::Nkey {
            nkey: NatsAuthNKey {
                nkey: "UAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                seed: "SBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into(),
            },
        }),
        request: Default::default(),
        jetstream: false,
    };

    let r = publish_and_check(conf).await;
    assert!(
        matches!(r, Err(NatsError::Connect { .. })),
        "publish_and_check failed, expected NatsError::Config, got: {:?}",
        r
    );
}

#[tokio::test]
async fn nats_tls_valid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url =
        std::env::var("NATS_TLS_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: Some(TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                ca_file: Some("tests/data/nats/rootCA.pem".into()),
                ..Default::default()
            },
        }),
        auth: None,
        request: Default::default(),
        jetstream: false,
    };

    let r = publish_and_check(conf).await;
    assert!(
        r.is_ok(),
        "publish_and_check failed, expected Ok(()), got: {:?}",
        r
    );
}

#[tokio::test]
async fn nats_tls_invalid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url =
        std::env::var("NATS_TLS_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: None,
        auth: None,
        request: Default::default(),
        jetstream: false,
    };

    let r = publish_and_check(conf).await;
    assert!(
        matches!(r, Err(NatsError::Connect { .. })),
        "publish_and_check failed, expected NatsError::Connect, got: {:?}",
        r
    );
}

#[tokio::test]
async fn nats_tls_client_cert_valid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_TLS_CLIENT_CERT_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: Some(TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                ca_file: Some("tests/data/nats/rootCA.pem".into()),
                crt_file: Some("tests/data/nats/nats-client.pem".into()),
                key_file: Some("tests/data/nats/nats-client.key".into()),
                ..Default::default()
            },
        }),
        auth: None,
        request: Default::default(),
        jetstream: false,
    };

    let r = publish_and_check(conf).await;
    assert!(
        r.is_ok(),
        "publish_and_check failed, expected Ok(()), got: {:?}",
        r
    );
}

#[tokio::test]
async fn nats_tls_client_cert_invalid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_TLS_CLIENT_CERT_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: Some(TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                ca_file: Some("tests/data/nats/rootCA.pem".into()),
                ..Default::default()
            },
        }),
        auth: None,
        request: Default::default(),
        jetstream: false,
    };

    let r = publish_and_check(conf).await;
    assert!(
        matches!(r, Err(NatsError::Connect { .. })),
        "publish_and_check failed, expected NatsError::Connect, got: {:?}",
        r
    );
}

#[tokio::test]
async fn nats_tls_jwt_auth_valid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url =
        std::env::var("NATS_JWT_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: Some(TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                ca_file: Some("tests/data/nats/rootCA.pem".into()),
                ..Default::default()
            },
        }),
        auth: Some(NatsAuthConfig::CredentialsFile {
            credentials_file: NatsAuthCredentialsFile {
                path: "tests/data/nats/nats.creds".into(),
            },
        }),
        request: Default::default(),
        jetstream: false,
    };

    let r = publish_and_check(conf).await;
    assert!(
        r.is_ok(),
        "publish_and_check failed, expected Ok(()), got: {:?}",
        r
    );
}

#[tokio::test]
async fn nats_tls_jwt_auth_invalid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url =
        std::env::var("NATS_JWT_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject.as_str()).unwrap(),
        url,
        tls: Some(TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                ca_file: Some("tests/data/nats/rootCA.pem".into()),
                ..Default::default()
            },
        }),
        auth: Some(NatsAuthConfig::CredentialsFile {
            credentials_file: NatsAuthCredentialsFile {
                path: "tests/data/nats/nats-bad.creds".into(),
            },
        }),
        request: Default::default(),
        jetstream: false,
    };

    let r = publish_and_check(conf).await;
    assert!(
        matches!(r, Err(NatsError::Connect { .. })),
        "publish_and_check failed, expected NatsError::Connect, got: {:?}",
        r
    );
}
