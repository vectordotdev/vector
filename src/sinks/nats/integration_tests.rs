use super::{
    config::{NatsHeaderConfig, NatsSinkConfig},
    sink::NatsSink,
    ConfigSnafu, NatsError,
};
use crate::{
    nats::{
        NatsAuthConfig, NatsAuthCredentialsFile, NatsAuthNKey, NatsAuthToken, NatsAuthUserPassword,
    },
    sinks::nats::config::JetStreamConfig,
    sinks::prelude::*,
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        random_lines_with_stream, random_string, trace_init,
    },
    tls::TlsEnableableConfig,
};
use async_nats::jetstream::stream::StorageType;
use futures_util::StreamExt;
use serde::Deserialize;
use snafu::ResultExt;
use std::time::Duration;
use vector_lib::codecs::{JsonSerializerConfig, TextSerializerConfig};
use vector_lib::event::{EventArray, LogEvent};
use vrl::value;

fn generate_sink_config(url: &str, subject: &str) -> NatsSinkConfig {
    NatsSinkConfig {
        acknowledgements: Default::default(),
        encoding: TextSerializerConfig::default().into(),
        connection_name: "".to_owned(),
        subject: Template::try_from(subject).unwrap(),
        url: url.to_string(),
        tls: None,
        auth: None,
        request: Default::default(),
        jetstream: false.into(),
    }
}

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
    let options: async_nats::ConnectOptions = (&conf).try_into().context(ConfigSnafu)?;
    let consumer = conf
        .clone()
        .connect(options)
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

    let conf = generate_sink_config(&url, &subject);

    let r = publish_and_check(conf).await;
    assert!(
        r.is_ok(),
        "publish_and_check failed, expected Ok(()), got: {r:?}"
    );
}

#[tokio::test]
async fn nats_userpass_auth_valid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_USERPASS_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_sink_config(&url, &subject);
    conf.auth = Some(NatsAuthConfig::UserPassword {
        user_password: NatsAuthUserPassword {
            user: "natsuser".to_string(),
            password: "natspass".to_string().into(),
        },
    });

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

    let mut conf = generate_sink_config(&url, &subject);
    conf.auth = Some(NatsAuthConfig::UserPassword {
        user_password: NatsAuthUserPassword {
            user: "natsuser".to_string(),
            password: "wrongpass".to_string().into(),
        },
    });

    let r = publish_and_check(conf).await;
    assert!(
        matches!(r, Err(NatsError::Connect { .. })),
        "publish_and_check failed, expected NatsError::Connect, got: {r:?}"
    );
}

#[tokio::test]
async fn nats_token_auth_valid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_TOKEN_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_sink_config(&url, &subject);
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
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_TOKEN_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_sink_config(&url, &subject);
    conf.auth = Some(NatsAuthConfig::Token {
        token: NatsAuthToken {
            value: "wrongsecret".to_string().into(),
        },
    });

    let r = publish_and_check(conf).await;
    assert!(
        matches!(r, Err(NatsError::Connect { .. })),
        "publish_and_check failed, expected NatsError::Connect, got: {r:?}"
    );
}

#[tokio::test]
async fn nats_nkey_auth_valid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_NKEY_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_sink_config(&url, &subject);
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
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_NKEY_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_sink_config(&url, &subject);
    conf.auth = Some(NatsAuthConfig::Nkey {
        nkey: NatsAuthNKey {
            nkey: "UAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            seed: "SBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into(),
        },
    });

    let r = publish_and_check(conf).await;
    assert!(
        matches!(r, Err(NatsError::Connect { .. })),
        "publish_and_check failed, expected NatsError::Config, got: {r:?}"
    );
}

#[tokio::test]
async fn nats_tls_valid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url =
        std::env::var("NATS_TLS_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_sink_config(&url, &subject);
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
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url =
        std::env::var("NATS_TLS_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let conf = generate_sink_config(&url, &subject);

    let r = publish_and_check(conf).await;
    assert!(
        matches!(r, Err(NatsError::Connect { .. })),
        "publish_and_check failed, expected NatsError::Connect, got: {r:?}"
    );
}

#[tokio::test]
async fn nats_tls_client_cert_valid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_TLS_CLIENT_CERT_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_sink_config(&url, &subject);
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
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url = std::env::var("NATS_TLS_CLIENT_CERT_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_sink_config(&url, &subject);
    conf.tls = Some(TlsEnableableConfig {
        enabled: Some(true),
        options: TlsConfig {
            ca_file: Some("tests/data/nats/rootCA.pem".into()),
            ..Default::default()
        },
    });

    let r = publish_and_check(conf).await;
    assert!(
        matches!(r, Err(NatsError::Connect { .. })),
        "publish_and_check failed, expected NatsError::Connect, got: {r:?}"
    );
}

#[tokio::test]
async fn nats_tls_jwt_auth_valid() {
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url =
        std::env::var("NATS_JWT_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_sink_config(&url, &subject);
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
    trace_init();

    let subject = format!("test-{}", random_string(10));
    let url =
        std::env::var("NATS_JWT_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let mut conf = generate_sink_config(&url, &subject);
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
        matches!(r, Err(NatsError::Config { .. })),
        "publish_and_check failed, expected NatsError::Config, got: {r:?}"
    );
}

#[tokio::test]
async fn nats_jetstream_valid() {
    trace_init();
    let stream_name = format!("EVENTS_{}", random_string(10));
    let subject = format!("events.test-{}", random_string(10));

    let url = std::env::var("NATS_JETSTREAM_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

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
    .expect("Failed to create JetStream stream");

    let mut conf = generate_sink_config(&url, &subject);
    conf.jetstream = JetStreamConfig {
        enabled: true,
        ..Default::default()
    };

    let r = publish_and_check(conf).await;
    assert!(
        r.is_ok(),
        "publish_and_check failed, expected Ok(()), got: {r:?}"
    );
}

#[derive(Debug, Deserialize)]
struct JetstreamTestEvent {
    id: String,
    message: String,
}

// Check that message ids are dynamically assigned
// and that they are correctly deduplicated at the stream level
#[tokio::test]
async fn nats_jetstream_message_id_valid() {
    trace_init();
    let stream_name = format!("EVENTS_MSG_ID_{}", random_string(10));
    let subject = format!("events.test.msg.id-{}", random_string(10));

    let url = std::env::var("NATS_JETSTREAM_ADDRESS")
        .unwrap_or_else(|_| String::from("nats://localhost:4222"));

    let client = async_nats::connect(&url).await.unwrap();
    let js = async_nats::jetstream::new(client);
    js.get_or_create_stream(async_nats::jetstream::stream::Config {
        name: stream_name.clone(),
        subjects: vec![subject.clone()],
        storage: StorageType::Memory,
        duplicate_window: std::time::Duration::from_secs(60),
        ..Default::default()
    })
    .await
    .unwrap();

    let mut conf = generate_sink_config(&url, &subject);
    conf.encoding = JsonSerializerConfig::default().into();

    let header_config = NatsHeaderConfig {
        message_id: Some(Template::try_from("{{ id }}").unwrap()),
    };

    conf.jetstream = JetStreamConfig {
        enabled: true,
        headers: Some(header_config),
    };

    let sink = NatsSink::new(conf.clone()).await.unwrap();
    let sink = VectorSink::from_event_streamsink(sink);

    let event_id = "123";

    let event1 = LogEvent::from(value!({
        "id": event_id,
        "message": "first message",
    }));

    let event2 = LogEvent::from(value!({
        "id": event_id,
        "message": "second message",
    }));

    let event3 = LogEvent::from(value!({
        "id": event_id,
        "message": "third message",
    }));

    let event_array = EventArray::Logs(vec![event1, event2, event3]);
    sink.run(futures::stream::iter(vec![event_array]).boxed())
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let stream = js.get_stream(&stream_name).await.unwrap();
    let consumer = stream
        .create_consumer(async_nats::jetstream::consumer::pull::Config {
            durable_name: Some(format!("test-consumer-{}", random_string(5))),
            ..Default::default()
        })
        .await
        .unwrap();

    let mut processed_messages = Vec::new();

    let messages = consumer.fetch().max_messages(2).messages().await;
    let mut stream = messages.expect("Failed to get stream");

    while let Some(Ok(msg)) = stream.next().await {
        msg.ack().await.unwrap();
        processed_messages.push(msg);
    }

    assert_eq!(
        processed_messages.len(),
        1,
        "Expected only one message due to deduplication"
    );

    let msg = &processed_messages[0];
    let received_event: JetstreamTestEvent = serde_json::from_slice(&msg.payload).unwrap();
    assert_eq!(received_event.id, event_id);
    assert_eq!(received_event.message, "first message");
}
