use bytes::Bytes;
use chrono::Utc;
use futures::{pin_mut, stream, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::FramedRead;
use vector_core::ByteSizeOf;

use crate::{
    codecs::{
        self,
        decoding::{DecodingConfig, DeserializerConfig, FramingConfig},
    },
    config::{
        log_schema, DataType, GenerateConfig, Output, SourceConfig, SourceContext,
        SourceDescription,
    },
    event::Event,
    internal_events::{BytesReceived, NatsEventsReceived, StreamClosedError},
    nats::{from_tls_auth_config, NatsAuthConfig, NatsConfigError},
    serde::{default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    sources::util::StreamDecodingError,
    tls::TlsConfig,
    SourceSender,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("NATS Config Error: {}", source))]
    Config { source: NatsConfigError },
    #[snafu(display("NATS Connect Error: {}", source))]
    Connect { source: std::io::Error },
    #[snafu(display("NATS Subscribe Error: {}", source))]
    Subscribe { source: std::io::Error },
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
struct NatsSourceConfig {
    url: String,
    #[serde(alias = "name")]
    connection_name: String,
    subject: String,
    queue: Option<String>,
    tls: Option<TlsConfig>,
    #[serde(flatten)]
    auth: Option<NatsAuthConfig>,
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    framing: FramingConfig,
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    decoding: DeserializerConfig,
}

inventory::submit! {
    SourceDescription::new::<NatsSourceConfig>("nats")
}

impl GenerateConfig for NatsSourceConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            connection_name = "vector"
            subject = "from.vector"
            url = "nats://127.0.0.1:4222""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "nats")]
impl SourceConfig for NatsSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let (connection, subscription) = create_subscription(self).await?;
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build();

        Ok(Box::pin(nats_source(
            connection,
            subscription,
            decoder,
            cx.shutdown,
            cx.out,
        )))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "nats"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

impl NatsSourceConfig {
    async fn connect(&self) -> Result<nats::asynk::Connection, BuildError> {
        let options: nats::asynk::Options = self.try_into().context(ConfigSnafu)?;
        options.connect(&self.url).await.context(ConnectSnafu)
    }
}

impl std::convert::TryFrom<&NatsSourceConfig> for nats::asynk::Options {
    type Error = NatsConfigError;

    fn try_from(config: &NatsSourceConfig) -> Result<Self, Self::Error> {
        from_tls_auth_config(&config.connection_name, &config.auth, &config.tls)
    }
}

fn get_subscription_stream(
    subscription: nats::asynk::Subscription,
) -> impl Stream<Item = nats::asynk::Message> {
    stream::unfold(subscription, |subscription| async move {
        subscription.next().await.map(|msg| (msg, subscription))
    })
}

async fn nats_source(
    // Take ownership of the connection so it doesn't get dropped.
    _connection: nats::asynk::Connection,
    subscription: nats::asynk::Subscription,
    decoder: codecs::Decoder,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let stream = get_subscription_stream(subscription).take_until(shutdown);
    pin_mut!(stream);
    while let Some(msg) = stream.next().await {
        emit!(&BytesReceived {
            byte_size: msg.data.len(),
            protocol: "tcp",
        });
        let mut stream = FramedRead::new(msg.data.as_ref(), decoder.clone());
        while let Some(next) = stream.next().await {
            match next {
                Ok((events, _byte_size)) => {
                    let count = events.len();
                    emit!(&NatsEventsReceived {
                        byte_size: events.size_of(),
                        count
                    });

                    let now = Utc::now();

                    let events = events.into_iter().map(|mut event| {
                        if let Event::Log(ref mut log) = event {
                            log.try_insert(log_schema().source_type_key(), Bytes::from("nats"));
                            log.try_insert(log_schema().timestamp_key(), now);
                        }
                        event
                    });

                    out.send_batch(events).await.map_err(|error| {
                        emit!(&StreamClosedError { error, count });
                    })?;
                }
                Err(error) => {
                    // Error is logged by `crate::codecs::Decoder`, no further
                    // handling is needed here.
                    if !error.can_continue() {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn create_subscription(
    config: &NatsSourceConfig,
) -> Result<(nats::asynk::Connection, nats::asynk::Subscription), BuildError> {
    let nc = config.connect().await?;

    let subscription = match &config.queue {
        None => nc.subscribe(&config.subject).await,
        Some(queue) => nc.queue_subscribe(&config.subject, queue).await,
    };

    let subscription = subscription.context(SubscribeSnafu)?;

    Ok((nc, subscription))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::print_stdout)] //tests

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<NatsSourceConfig>();
    }
}

#[cfg(feature = "nats-integration-tests")]
#[cfg(test)]
mod integration_tests {
    #![allow(clippy::print_stdout)] //tests

    use super::*;
    use crate::nats::{
        NatsAuthCredentialsFile, NatsAuthNKey, NatsAuthStrategy, NatsAuthToken,
        NatsAuthUserPassword,
    };
    use crate::test_util::{collect_n, random_string};
    use crate::tls::TlsOptions;

    async fn publish_and_check(conf: NatsSourceConfig) -> Result<(), BuildError> {
        let subject = conf.subject.clone();
        let (nc, sub) = create_subscription(&conf).await?;
        let nc_pub = nc.clone();

        let (tx, rx) = SourceSender::new_test();
        let decoder = DecodingConfig::new(conf.framing.clone(), conf.decoding.clone()).build();
        tokio::spawn(nats_source(nc, sub, decoder, ShutdownSignal::noop(), tx));
        let msg = "my message";
        nc_pub.publish(&subject, msg).await.unwrap();

        let events = collect_n(rx, 1).await;
        println!("Received event  {:?}", events[0].as_log());
        assert_eq!(events[0].as_log()[log_schema().message_key()], msg.into());
        Ok(())
    }

    #[tokio::test]
    async fn nats_no_auth() {
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4222".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: None,
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
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4223".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: Some(NatsAuthConfig {
                strategy: NatsAuthStrategy::UserPassword,
                user_password: Some(NatsAuthUserPassword {
                    user: "natsuser".into(),
                    password: "natspass".into(),
                }),
                ..Default::default()
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_userpass_auth_invalid() {
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4223".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: Some(NatsAuthConfig {
                strategy: NatsAuthStrategy::UserPassword,
                user_password: Some(NatsAuthUserPassword {
                    user: "natsuser".into(),
                    password: "wrongpass".into(),
                }),
                ..Default::default()
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_token_auth_valid() {
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4224".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: Some(NatsAuthConfig {
                strategy: NatsAuthStrategy::Token,
                token: Some(NatsAuthToken {
                    value: "secret".into(),
                }),
                ..Default::default()
            }),
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
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4224".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: Some(NatsAuthConfig {
                strategy: NatsAuthStrategy::Token,
                token: Some(NatsAuthToken {
                    value: "wrongsecret".into(),
                }),
                ..Default::default()
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_nkey_auth_valid() {
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4225".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: Some(NatsAuthConfig {
                strategy: NatsAuthStrategy::NKey,
                nkey: Some(NatsAuthNKey {
                    nkey: "UD345ZYSUJQD7PNCTWQPINYSO3VH4JBSADBSYUZOBT666DRASFRAWAWT".into(),
                    seed: "SUANIRXEZUROTXNFN3TJYMT27K7ZZVMD46FRIHF6KXKS4KGNVBS57YAFGY".into(),
                }),
                ..Default::default()
            }),
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
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4225".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: Some(NatsAuthConfig {
                strategy: NatsAuthStrategy::NKey,
                nkey: Some(NatsAuthNKey {
                    nkey: "UAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                    seed: "SBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into(),
                }),
                ..Default::default()
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Config { .. })),
            "publish_and_check failed, expected BuildError::Config, got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_tls_valid() {
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4227".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: Some(TlsConfig {
                enabled: Some(true),
                options: TlsOptions {
                    ca_file: Some("tests/data/mkcert_rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: None,
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
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4227".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: None,
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_tls_client_cert_valid() {
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4228".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: Some(TlsConfig {
                enabled: Some(true),
                options: TlsOptions {
                    ca_file: Some("tests/data/mkcert_rootCA.pem".into()),
                    crt_file: Some("tests/data/nats_client_cert.pem".into()),
                    key_file: Some("tests/data/nats_client_key.pem".into()),
                    ..Default::default()
                },
            }),
            auth: None,
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
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4228".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: Some(TlsConfig {
                enabled: Some(true),
                options: TlsOptions {
                    ca_file: Some("tests/data/mkcert_rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: None,
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_tls_jwt_auth_valid() {
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4229".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: Some(TlsConfig {
                enabled: Some(true),
                options: TlsOptions {
                    ca_file: Some("tests/data/mkcert_rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: Some(NatsAuthConfig {
                strategy: NatsAuthStrategy::NKey,
                credentials_file: Some(NatsAuthCredentialsFile {
                    path: "tests/data/nats.creds".into(),
                }),
                ..Default::default()
            }),
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
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4229".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: Some(TlsConfig {
                enabled: Some(true),
                options: TlsOptions {
                    ca_file: Some("tests/data/mkcert_rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: Some(NatsAuthConfig {
                strategy: NatsAuthStrategy::NKey,
                credentials_file: Some(NatsAuthCredentialsFile {
                    path: "tests/data/nats-bad.creds".into(),
                }),
                ..Default::default()
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {:?}",
            r
        );
    }
}
