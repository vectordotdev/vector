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

fn generate_source_config(url: &str, subject: &str) -> NatsSourceConfig {
    NatsSourceConfig {
        url: url.to_string(),
        connection_name: format!("vector-test-{}", subject),
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
    let subject = format!("test-js-{}", random_string(10));
    let url = std::env::var("NATS_JETSTREAM_ADDRESS")
        .unwrap_or_else(|_| "nats://localhost:4222".to_string());

    let mut conf = generate_source_config(&url, &subject);
    conf.jetstream = Some(JetStreamConfig {
        stream: format!("S_{}", subject.replace('.', "_")),
        consumer: format!("C_{}", subject.replace('.', "_")),
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
    let subject = format!("test-js-no-consumer-{}", random_string(10));
    let url = std::env::var("NATS_JETSTREAM_ADDRESS")
        .unwrap_or_else(|_| "nats://localhost:4222".to_string());

    let stream_name = format!("S_{}", subject.replace('.', "_"));

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

#[tokio::test]
async fn nats_slow_consumer_event() {
    use futures::StreamExt;
    use tokio::time::{Duration, sleep};

    vector_lib::metrics::init_test();

    let subject = format!("test-slow-consumer-{}", random_string(10));
    let url =
        std::env::var("NATS_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_source_config(&url, &subject);
    // Set subscriber capacity to 1 to trigger slow consumer events quickly
    conf.subscriber_capacity = 1;

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

    let (shutdown_trigger, shutdown_signal, shutdown_done) = ShutdownSignal::new_wired();

    let source_handle = tokio::spawn(run_nats_core(
        conf.clone(),
        nc,
        sub,
        decoder,
        LogNamespace::Legacy,
        shutdown_signal,
        tx,
    ));

    // Publish many messages rapidly to overwhelm the subscriber buffer (capacity = 1)
    for i in 0..100 {
        nc_pub
            .publish(subject.clone(), Bytes::from(format!("msg{}", i)))
            .await
            .unwrap();
    }
    nc_pub.flush().await.unwrap();

    // Give the source time to process and trigger slow consumer events
    sleep(Duration::from_millis(200)).await;

    // Drain messages from the receiver with a timeout to prevent blocking
    let mut message_count = 0;
    let drain_timeout = Duration::from_secs(2);
    let start = tokio::time::Instant::now();

    while message_count < 50 && start.elapsed() < drain_timeout {
        if tokio::time::timeout(Duration::from_millis(50), rx.next())
            .await
            .is_ok()
        {
            message_count += 1;
        } else {
            break;
        }
    }

    // Check that the slow consumer metric was incremented
    let controller = vector_lib::metrics::Controller::get()
        .expect("Metrics controller should be initialized");
    let metrics = controller.capture_metrics();
    let slow_consumer_metric = metrics
        .iter()
        .find(|m| m.name() == "nats_slow_consumer_events_total");

    assert!(
        slow_consumer_metric.is_some(),
        "Expected nats_slow_consumer_events_total metric to be emitted"
    );

    if let Some(metric) = slow_consumer_metric {
        // Check the counter value
        let value = match metric.value() {
            vector_lib::event::MetricValue::Counter { value } => value,
            _ => panic!("Expected counter metric"),
        };
        assert!(
            *value > 0.0,
            "Expected nats_slow_consumer_events_total to be > 0, got {}",
            value
        );
        println!("Slow consumer events recorded: {}", value);

        // Verify component_id label is present and correct
        let tags = metric.tags().expect("Metric should have tags");
        let component_id_tag = tags.get("component_id");
        assert!(
            component_id_tag.is_some(),
            "Expected component_id tag in metric"
        );
        assert_eq!(
            component_id_tag.unwrap(),
            &conf.connection_name,
            "component_id should match connection_name"
        );
        println!(
            "Component ID correctly set to: {}",
            component_id_tag.unwrap()
        );

        // Verify subscription_id label is present
        let subscription_id_tag = tags.get("subscription_id");
        assert!(
            subscription_id_tag.is_some(),
            "Expected subscription_id tag in metric"
        );
        println!(
            "Subscription ID captured: {}",
            subscription_id_tag.unwrap()
        );
    }

    // Cleanup with timeout to prevent test from hanging
    shutdown_trigger.cancel();

    tokio::time::timeout(Duration::from_secs(5), async {
        source_handle.await.unwrap().expect("Source task failed");
        shutdown_done.await;
    })
    .await
    .expect("Test timed out during cleanup");
}
