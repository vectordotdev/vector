#![allow(clippy::print_stdout)]
use async_nats::jetstream::stream::StorageType;
use bytes::Bytes;
use vector_lib::config::log_schema;

use crate::{
    SourceSender,
    codecs::DecodingConfig,
    config::{LogNamespace, SourceConfig, SourceContext},
    nats::{
        NatsAuthConfig, NatsAuthCredentialsFile, NatsAuthNKey, NatsAuthToken, NatsAuthUserPassword,
    },
    serde::{default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    sources::nats::{
        config::{BuildError, JetStreamConfig, NatsSourceConfig, default_subject_key_field},
        source::{create_subscription, run_nats_core},
    },
    test_util::{
        collect_n,
        components::{SOURCE_TAGS, assert_source_compliance},
        random_string,
    },
    tls::{TlsConfig, TlsEnableableConfig},
};

/// Generates a random test identifier safe for use as a NATS subject,
/// stream name, and consumer name (alphanumeric + underscores only).
fn random_jetstream_id(prefix: &str) -> (String, String, String) {
    let id = random_string(10);
    let subject = format!("{prefix}_{id}");
    let stream_name = format!("S_{prefix}_{id}");
    let consumer_name = format!("C_{prefix}_{id}");
    (subject, stream_name, consumer_name)
}

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

/// Test runner for JetStream sources.
/// This function sets up the required JetStream stream and consumer,
/// publishes a message, and then runs the source to ensure it receives the message.
async fn run_jetstream_test(conf: NatsSourceConfig) -> Result<(), crate::Error> {
    let js_config = conf.jetstream.clone().unwrap();
    let subject = conf.subject.clone();
    let msg = "my jetstream message";

    // Connect to NATS and set up the JetStream stream and consumer.
    let client = async_nats::connect(conf.url.clone())
        .await
        .expect("Failed to connect to NATS");
    let js = async_nats::jetstream::new(client.clone());

    js.get_or_create_stream(async_nats::jetstream::stream::Config {
        name: js_config.stream.clone(),
        subjects: vec![subject.clone()],
        storage: StorageType::Memory,
        ..Default::default()
    })
    .await
    .expect("Failed to create stream");

    let stream = js.get_stream(js_config.stream).await.unwrap();
    stream
        .create_consumer(async_nats::jetstream::consumer::pull::Config {
            durable_name: Some(js_config.consumer),
            ..Default::default()
        })
        .await
        .unwrap();

    // Publish a message for the source to consume.
    js.publish(subject, msg.as_bytes().into()).await.unwrap();

    // Run the source and verify it receives the event.
    let events = assert_source_compliance(&SOURCE_TAGS, async move {
        let (tx, rx) = SourceSender::new_test();
        let cx = SourceContext::new_test(tx, None);
        let source = conf.build(cx).await.unwrap();

        tokio::spawn(source);

        collect_n(rx, 1).await
    })
    .await;

    assert_eq!(
        events[0].as_log()[log_schema().message_key().unwrap().to_string()],
        msg.into()
    );

    Ok(())
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
    let url =
        std::env::var("NATS_TLS_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_source_config(&url, &subject);
    conf.tls = Some(TlsEnableableConfig {
        enabled: Some(true),
        options: TlsConfig {
            ca_file: Some("tests/integration/nats/data/rootCA.pem".into()),
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
    let url =
        std::env::var("NATS_TLS_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

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
            ca_file: Some("tests/integration/nats/data/rootCA.pem".into()),
            crt_file: Some("tests/integration/nats/data/nats-client.pem".into()),
            key_file: Some("tests/integration/nats/data/nats-client.key".into()),
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
            ca_file: Some("tests/integration/nats/data/rootCA.pem".into()),
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
    let url =
        std::env::var("NATS_JWT_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_source_config(&url, &subject);
    conf.tls = Some(TlsEnableableConfig {
        enabled: Some(true),
        options: TlsConfig {
            ca_file: Some("tests/integration/nats/data/rootCA.pem".into()),
            ..Default::default()
        },
    });
    conf.auth = Some(NatsAuthConfig::CredentialsFile {
        credentials_file: NatsAuthCredentialsFile {
            path: "tests/integration/nats/data/nats.creds".into(),
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
    let url =
        std::env::var("NATS_JWT_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_source_config(&url, &subject);
    conf.tls = Some(TlsEnableableConfig {
        enabled: Some(true),
        options: TlsConfig {
            ca_file: Some("tests/integration/nats/data/rootCA.pem".into()),
            ..Default::default()
        },
    });
    conf.auth = Some(NatsAuthConfig::CredentialsFile {
        credentials_file: NatsAuthCredentialsFile {
            path: "tests/integration/nats/data/nats-bad.creds".into(),
        },
    });

    let r = publish_and_check(conf).await;
    assert!(
        matches!(r, Err(BuildError::Config { .. })),
        "publish_and_check failed, expected BuildError::Config, got: {r:?}"
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

#[tokio::test]
async fn nats_jetstream_valid() {
    let (subject, stream_name, consumer_name) = random_jetstream_id("test_js");
    let url = std::env::var("NATS_JETSTREAM_ADDRESS")
        .unwrap_or_else(|_| "nats://localhost:4222".to_string());

    let mut conf = generate_source_config(&url, &subject);
    conf.jetstream = Some(JetStreamConfig {
        stream: stream_name,
        consumer: consumer_name,
        ..Default::default()
    });

    let result = run_jetstream_test(conf).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn nats_jetstream_stream_not_found() {
    let subject = format!("test-js-no-stream-{}", random_string(10));
    let url = std::env::var("NATS_JETSTREAM_ADDRESS")
        .unwrap_or_else(|_| "nats://localhost:4222".to_string());

    let mut conf = generate_source_config(&url, &subject);
    conf.jetstream = Some(JetStreamConfig {
        stream: "nonexistent-stream".to_string(),
        consumer: "nonexistent-consumer".to_string(),
        ..Default::default()
    });

    let (tx, _rx) = SourceSender::new_test();
    let cx = SourceContext::new_test(tx, None);
    let result = conf.build(cx).await;

    match result {
        Ok(_) => panic!("Test failed: expected an error but got Ok"),
        Err(err) => {
            let build_err = err.downcast_ref::<BuildError>().unwrap();
            assert!(matches!(build_err, BuildError::Stream { .. }));
        }
    }
}

#[tokio::test]
async fn nats_jetstream_consumer_not_found() {
    let (subject, stream_name, _consumer_name) = random_jetstream_id("test_js_no_consumer");
    let url = std::env::var("NATS_JETSTREAM_ADDRESS")
        .unwrap_or_else(|_| "nats://localhost:4222".to_string());

    // Setup: Create the stream but NOT the consumer.
    let client = async_nats::connect(&url).await.unwrap();
    let js = async_nats::jetstream::new(client);
    js.get_or_create_stream(async_nats::jetstream::stream::Config {
        name: stream_name.clone(),
        subjects: vec![subject.clone()],
        storage: StorageType::Memory,
        ..Default::default()
    })
    .await
    .unwrap();

    let mut conf = generate_source_config(&url, &subject);
    conf.jetstream = Some(JetStreamConfig {
        stream: stream_name,
        consumer: "nonexistent-consumer".to_string(),
        ..Default::default()
    });

    let (tx, _rx) = SourceSender::new_test();
    let cx = SourceContext::new_test(tx, None);
    let result = conf.build(cx).await;

    match result {
        Ok(_) => panic!("Test failed: expected an error but got Ok"),
        Err(err) => {
            let build_err = err.downcast_ref::<BuildError>().unwrap();
            assert!(matches!(build_err, BuildError::Consumer { .. }));
        }
    }
}

#[tokio::test]
async fn nats_shutdown_drain_messages() {
    use futures::StreamExt;
    use tokio::time::{Duration, timeout};

    let subject = format!("test-drain-{}", random_string(10));
    let url =
        std::env::var("NATS_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));
    let conf = generate_source_config(&url, &subject);

    let (shutdown_trigger, shutdown_signal, shutdown_done) = ShutdownSignal::new_wired();

    let (nc, sub) = create_subscription(&conf).await.unwrap();
    let nc_pub = nc.clone();
    let (tx, mut rx) = SourceSender::new_test();
    let decoder = DecodingConfig::new(
        conf.framing.clone(),
        conf.decoding.clone(),
        LogNamespace::Legacy,
    )
    .build()
    .unwrap();

    let source_handle = tokio::spawn(run_nats_core(
        conf.clone(),
        nc,
        sub,
        decoder,
        LogNamespace::Legacy,
        shutdown_signal,
        tx,
    ));

    nc_pub
        .publish(subject.clone(), Bytes::from_static(b"msg1"))
        .await
        .unwrap();
    nc_pub
        .publish(subject.clone(), Bytes::from_static(b"msg2"))
        .await
        .unwrap();
    nc_pub
        .publish(subject.clone(), Bytes::from_static(b"msg3"))
        .await
        .unwrap();

    // Ensure the messages are sent to the server before we trigger the shutdown
    nc_pub.flush().await.unwrap();

    // Trigger the graceful shutdown
    shutdown_trigger.cancel();

    // Publish another message *after* shutdown. This should be ignored by the draining source.
    nc_pub
        .publish(subject.clone(), Bytes::from_static(b"ignored"))
        .await
        .unwrap();
    nc_pub.flush().await.unwrap();

    let mut events = Vec::new();
    for _ in 0..3 {
        let event = timeout(Duration::from_secs(5), rx.next())
            .await
            .expect("Test timed out waiting for drained messages.")
            .expect("Stream ended before all messages were drained.");
        events.push(event);
    }
    assert_eq!(events.len(), 3);
    let msg = &events[0].as_log()[log_schema().message_key().unwrap().to_string()];
    assert_eq!(*msg, "msg1".into());

    // Verify the source has completed its work and the shutdown is fully done.
    source_handle.await.unwrap().expect("Source task failed");
    shutdown_done.await;
}

/// Test harness for JetStream recovery tests.
///
/// Encapsulates NATS JetStream setup (stream, consumer, source) and provides
/// ergonomic helpers for publish, disruption, and drain-until-found patterns.
struct JetStreamTestHarness {
    js: async_nats::jetstream::Context,
    rx: futures::stream::BoxStream<'static, crate::event::Event>,
    subject: String,
    stream_name: String,
    consumer_name: String,
    _source_handle: tokio::task::JoinHandle<Result<(), ()>>,
}

impl JetStreamTestHarness {
    /// Creates a new harness with a fresh stream, consumer, and running source.
    async fn new() -> Self {
        use futures::StreamExt;

        let url = std::env::var("NATS_JETSTREAM_ADDRESS")
            .unwrap_or_else(|_| "nats://localhost:4222".to_string());

        let (subject, stream_name, consumer_name) = random_jetstream_id("test_js");

        let client = async_nats::connect(&url)
            .await
            .expect("Failed to connect to NATS");
        let js = async_nats::jetstream::new(client);

        js.get_or_create_stream(async_nats::jetstream::stream::Config {
            name: stream_name.clone(),
            subjects: vec![subject.clone()],
            storage: StorageType::Memory,
            ..Default::default()
        })
        .await
        .expect("Failed to create stream");

        let stream = js.get_stream(&stream_name).await.unwrap();
        stream
            .create_consumer(async_nats::jetstream::consumer::pull::Config {
                durable_name: Some(consumer_name.clone()),
                ..Default::default()
            })
            .await
            .unwrap();

        let conf = NatsSourceConfig {
            url,
            subject: subject.clone(),
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            subject_key_field: default_subject_key_field(),
            jetstream: Some(JetStreamConfig {
                stream: stream_name.clone(),
                consumer: consumer_name.clone(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let (tx, rx) = SourceSender::new_test();
        let cx = SourceContext::new_test(tx, None);
        let source = conf.build(cx).await.unwrap();
        let source_handle = tokio::spawn(source);

        Self {
            js,
            rx: rx.boxed(),
            subject,
            stream_name,
            consumer_name,
            _source_handle: source_handle,
        }
    }

    /// Publishes a message and waits for it to be acked by the server.
    async fn publish(&self, payload: &str) {
        self.js
            .publish(self.subject.clone(), Bytes::from(payload.to_owned()))
            .await
            .unwrap()
            .await
            .unwrap();
    }

    /// Publishes a message and asserts it is eventually delivered to the source.
    ///
    /// Uses `drain_until` internally, so re-delivered messages are skipped.
    async fn publish_and_expect(&mut self, payload: &str) {
        self.publish(payload).await;
        assert!(
            self.drain_until(payload, 5).await,
            "Expected message '{payload}' not received within timeout"
        );
    }

    /// Deletes the durable consumer on the server, invalidating the source's
    /// pull stream. Waits 3 seconds for the pull loop to observe the error.
    async fn delete_consumer(&self) {
        let stream = self.js.get_stream(&self.stream_name).await.unwrap();
        stream
            .delete_consumer(&self.consumer_name)
            .await
            .expect("Failed to delete consumer");
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }

    /// Recreates the durable consumer so the recovery loop can reconnect.
    async fn recreate_consumer(&self) {
        let stream = self.js.get_stream(&self.stream_name).await.unwrap();
        stream
            .create_consumer(async_nats::jetstream::consumer::pull::Config {
                durable_name: Some(self.consumer_name.clone()),
                ..Default::default()
            })
            .await
            .expect("Failed to recreate consumer");
    }

    /// Deletes the entire JetStream stream (implicitly deletes all consumers).
    /// Waits 3 seconds for the pull loop to observe the error.
    async fn delete_stream(&self) {
        self.js
            .delete_stream(&self.stream_name)
            .await
            .expect("Failed to delete stream");
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }

    /// Recreates both the stream and consumer after a full stream deletion.
    async fn recreate_stream_and_consumer(&self) {
        self.js
            .get_or_create_stream(async_nats::jetstream::stream::Config {
                name: self.stream_name.clone(),
                subjects: vec![self.subject.clone()],
                storage: StorageType::Memory,
                ..Default::default()
            })
            .await
            .expect("Failed to recreate stream");
        self.recreate_consumer().await;
    }

    /// Drains events from the source output until a message with the given
    /// payload is found or the timeout expires. Returns `true` if found.
    ///
    /// Skips unrelated or re-delivered messages, which is important because
    /// newly created consumers may re-deliver previously seen messages.
    async fn drain_until(&mut self, target: &str, timeout_secs: u64) -> bool {
        use futures::StreamExt;
        use tokio::time::{Duration, timeout};

        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
        while tokio::time::Instant::now() < deadline {
            match timeout(Duration::from_secs(5), self.rx.next()).await {
                Ok(Some(event)) => {
                    let msg = event.as_log()[log_schema().message_key().unwrap().to_string()]
                        .to_string_lossy();
                    if msg == target {
                        return true;
                    }
                }
                Ok(None) => return false,
                Err(_) => return false,
            }
        }
        false
    }
}

/// Regression test for JetStream pull consumer becoming permanently stale after disruption.
///
/// Without the fix, the source silently exits on the first stream error.
/// With the fix, it retries with backoff and resumes delivery.
#[tokio::test]
async fn nats_jetstream_stale_consumer_after_close() {
    let mut h = JetStreamTestHarness::new().await;

    h.publish_and_expect("before-delete").await;

    h.delete_consumer().await;
    h.recreate_consumer().await;

    h.publish("after-delete").await;
    assert!(
        h.drain_until("after-delete", 10).await,
        "Source did not recover after consumer deletion — consumer is permanently stale"
    );
}

/// Verifies the source can recover multiple times in succession.
///
/// Deletes and recreates the consumer twice. After each recovery cycle,
/// a new message must be delivered, proving the retry loop is re-entrant.
#[tokio::test]
async fn nats_jetstream_multiple_recovery_cycles() {
    let mut h = JetStreamTestHarness::new().await;

    for cycle in 0..2 {
        let label_before = format!("cycle-{cycle}-before");
        let label_after = format!("cycle-{cycle}-after");

        h.publish_and_expect(&label_before).await;

        h.delete_consumer().await;
        h.recreate_consumer().await;

        h.publish(&label_after).await;
        assert!(
            h.drain_until(&label_after, 15).await,
            "Cycle {cycle}: source did not recover after consumer deletion"
        );
    }
}

/// Ensures the source shuts down cleanly while stuck in the recovery backoff loop.
///
/// After deleting the consumer (without recreating it), the source enters
/// the retry loop. Triggering shutdown must cause it to exit promptly.
#[tokio::test]
async fn nats_jetstream_shutdown_during_recovery() {
    use futures::StreamExt;
    use tokio::time::{Duration, timeout};

    use crate::{
        codecs::DecodingConfig,
        sources::nats::source::{create_consumer_stream, run_nats_jetstream},
    };

    let url = std::env::var("NATS_JETSTREAM_ADDRESS")
        .unwrap_or_else(|_| "nats://localhost:4222".to_string());

    let (subject, stream_name, consumer_name) = random_jetstream_id("test_shutdown");

    let setup_client = async_nats::connect(&url)
        .await
        .expect("Failed to connect to NATS for setup");
    let js = async_nats::jetstream::new(setup_client.clone());

    js.get_or_create_stream(async_nats::jetstream::stream::Config {
        name: stream_name.clone(),
        subjects: vec![subject.clone()],
        storage: StorageType::Memory,
        ..Default::default()
    })
    .await
    .expect("Failed to create stream");

    let stream = js.get_stream(&stream_name).await.unwrap();
    stream
        .create_consumer(async_nats::jetstream::consumer::pull::Config {
            durable_name: Some(consumer_name.clone()),
            ..Default::default()
        })
        .await
        .unwrap();

    let conf = NatsSourceConfig {
        url: url.clone(),
        subject: subject.clone(),
        framing: default_framing_message_based(),
        decoding: default_decoding(),
        subject_key_field: default_subject_key_field(),
        jetstream: Some(JetStreamConfig {
            stream: stream_name.clone(),
            consumer: consumer_name.clone(),
            ..Default::default()
        }),
        ..Default::default()
    };

    let (shutdown_trigger, shutdown_signal, shutdown_done) = ShutdownSignal::new_wired();

    let connection = conf.connect().await.unwrap();
    let js_config = conf.jetstream.clone().unwrap();
    let initial_messages = create_consumer_stream(&connection, &js_config)
        .await
        .unwrap();

    let decoder = DecodingConfig::new(
        conf.framing.clone(),
        conf.decoding.clone(),
        LogNamespace::Legacy,
    )
    .build()
    .unwrap();

    let (tx, mut rx) = SourceSender::new_test();

    let source_handle = tokio::spawn(run_nats_jetstream(
        conf.clone(),
        connection,
        initial_messages,
        decoder,
        LogNamespace::Legacy,
        shutdown_signal,
        tx,
    ));

    // Deliver one message to prove the source is running.
    js.publish(subject.clone(), Bytes::from_static(b"alive"))
        .await
        .unwrap()
        .await
        .unwrap();

    let event = timeout(Duration::from_secs(5), rx.next())
        .await
        .expect("Timed out waiting for initial message")
        .expect("Stream ended unexpectedly");
    assert_eq!(
        event.as_log()[log_schema().message_key().unwrap().to_string()],
        "alive".into()
    );

    // Delete consumer to force the source into the recovery loop.
    stream
        .delete_consumer(&consumer_name)
        .await
        .expect("Failed to delete consumer");
    // Do NOT recreate it — the source should keep retrying.

    // Wait for the source to enter the backoff loop.
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Signal shutdown.
    shutdown_trigger.cancel();

    // The source must exit within a reasonable time despite being in retry.
    let result = timeout(Duration::from_secs(10), source_handle)
        .await
        .expect("Source did not shut down within 10 seconds — stuck in recovery loop")
        .expect("Source task panicked");

    assert!(
        result.is_ok(),
        "Source should return Ok(()) on clean shutdown, got Err"
    );

    shutdown_done.await;
}

/// Verifies that messages published while the consumer is deleted (but the
/// stream still exists) are delivered after the consumer is recreated.
///
/// JetStream retains messages in the stream even when no consumer exists.
/// After recovery, the new consumer should pick up these queued messages.
#[tokio::test]
async fn nats_jetstream_messages_during_downtime() {
    use futures::StreamExt;
    use tokio::time::{Duration, timeout};

    let mut h = JetStreamTestHarness::new().await;

    h.publish_and_expect("initial").await;

    h.delete_consumer().await;

    // Publish messages while consumer is absent.
    // The stream still exists, so these are retained.
    for i in 0..5 {
        h.publish(&format!("queued-{i}")).await;
    }

    h.recreate_consumer().await;

    // Drain until we've found all five "queued-N" messages.
    let mut found = std::collections::HashSet::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while found.len() < 5 && tokio::time::Instant::now() < deadline {
        match timeout(Duration::from_secs(5), h.rx.next()).await {
            Ok(Some(event)) => {
                let msg = event.as_log()[log_schema().message_key().unwrap().to_string()]
                    .to_string_lossy()
                    .into_owned();
                if msg.starts_with("queued-") {
                    found.insert(msg);
                }
            }
            Ok(None) => panic!("Source stream ended — source exited instead of recovering"),
            Err(_) => break,
        }
    }

    assert_eq!(
        found.len(),
        5,
        "Expected all 5 queued messages after recovery, got {}: {:?}",
        found.len(),
        found
    );
}

/// Verifies recovery when the entire stream (not just the consumer) is deleted
/// and recreated. This tests a different server error path: `get_stream` fails
/// instead of `get_consumer`.
#[tokio::test]
async fn nats_jetstream_stream_deleted_and_recreated() {
    let mut h = JetStreamTestHarness::new().await;

    h.publish_and_expect("pre-stream-delete").await;

    h.delete_stream().await;
    h.recreate_stream_and_consumer().await;

    h.publish("post-stream-recreate").await;
    assert!(
        h.drain_until("post-stream-recreate", 15).await,
        "Source did not recover after stream deletion and recreation"
    );
}

/// Verifies the backoff resets after a successful recovery.
///
/// Performs two disruption cycles with a successful message delivery in between.
/// If the backoff did NOT reset, the second recovery would take much longer
/// due to accumulated delay. We enforce a tight timeout on the second cycle.
#[tokio::test]
async fn nats_jetstream_backoff_resets_after_recovery() {
    let mut h = JetStreamTestHarness::new().await;

    // --- First disruption cycle ---
    h.publish_and_expect("round1").await;

    h.delete_consumer().await;
    h.recreate_consumer().await;

    h.publish("after-round1").await;
    assert!(
        h.drain_until("after-round1", 15).await,
        "First recovery cycle failed"
    );

    // Deliver a few messages to prove steady state and reset backoff.
    for i in 0..3 {
        h.publish_and_expect(&format!("steady-{i}")).await;
    }

    // --- Second disruption cycle ---
    h.delete_consumer().await;
    h.recreate_consumer().await;

    let round2_start = tokio::time::Instant::now();
    h.publish("after-round2").await;

    // If backoff reset properly, recovery should happen within a few seconds
    // (initial backoff ~500ms). If it didn't reset, we'd be waiting 30+ seconds.
    assert!(
        h.drain_until("after-round2", 10).await,
        "Second recovery cycle failed — backoff may not have reset"
    );
    let round2_elapsed = round2_start.elapsed();
    assert!(
        round2_elapsed < std::time::Duration::from_secs(15),
        "Second recovery took {:?}, suggesting backoff did not reset",
        round2_elapsed
    );
}
