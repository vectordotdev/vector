use bytes::Bytes;
use chrono::Utc;
use futures::{pin_mut, stream, SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use tokio_util::codec::FramedRead;

use crate::{
    codecs::{
        self,
        decoding::{DecodingConfig, DeserializerConfig, FramingConfig},
    },
    config::{
        log_schema, DataType, GenerateConfig, SourceConfig, SourceContext, SourceDescription,
    },
    event::Event,
    internal_events::NatsEventsReceived,
    nats::{from_tls_auth_config, NatsAuthConfig, NatsConfigError},
    serde::{default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    sources::util::StreamDecodingError,
    tls::TlsConfig,
    Pipeline,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("NATS Config Error: {}", source))]
    Config { source: NatsConfigError },
    #[snafu(display("NATS Connection Error: {}", source))]
    Connection { source: std::io::Error },
    #[snafu(display("NATS Subscription Error: {}", source))]
    Subscription { source: std::io::Error },
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct NatsSourceConfig {
    url: String,
    #[serde(alias = "name")]
    connection_name: String,
    subject: String,
    queue: Option<String>,
    #[serde(default)]
    tls: Option<TlsConfig>,
    auth: Option<NatsAuthConfig>,
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    framing: Box<dyn FramingConfig>,
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    decoding: Box<dyn DeserializerConfig>,
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
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build()?;

        Ok(Box::pin(nats_source(
            connection,
            subscription,
            decoder,
            cx.shutdown,
            cx.out,
        )))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "nats"
    }
}

impl NatsSourceConfig {
    async fn connect(&self) -> Result<async_nats::Connection, BuildError> {
        let options: async_nats::Options = self
            .try_into()
            .map_err(|e| BuildError::Config { source: e })?;
        options
            .connect(&self.url)
            .await
            .map_err(|e| BuildError::Connection { source: e })
    }
}

impl std::convert::TryFrom<&NatsSourceConfig> for async_nats::Options {
    type Error = NatsConfigError;

    fn try_from(config: &NatsSourceConfig) -> Result<Self, Self::Error> {
        from_tls_auth_config(&config.connection_name, &config.auth, &config.tls)
    }
}

fn get_subscription_stream(
    subscription: async_nats::Subscription,
) -> impl Stream<Item = async_nats::Message> {
    stream::unfold(subscription, |subscription| async move {
        subscription.next().await.map(|msg| (msg, subscription))
    })
}

async fn nats_source(
    // Take ownership of the connection so it doesn't get dropped.
    _connection: async_nats::Connection,
    subscription: async_nats::Subscription,
    decoder: codecs::Decoder,
    shutdown: ShutdownSignal,
    mut out: Pipeline,
) -> Result<(), ()> {
    let stream = get_subscription_stream(subscription).take_until(shutdown);
    pin_mut!(stream);
    while let Some(msg) = stream.next().await {
        let mut stream = FramedRead::new(msg.data.as_ref(), decoder.clone());
        while let Some(next) = stream.next().await {
            match next {
                Ok((events, byte_size)) => {
                    emit!(&NatsEventsReceived {
                        byte_size,
                        count: events.len()
                    });

                    let now = Utc::now();

                    for mut event in events {
                        if let Event::Log(ref mut log) = event {
                            log.try_insert(log_schema().source_type_key(), Bytes::from("nats"));
                            log.try_insert(log_schema().timestamp_key(), now);
                        }

                        out.send(event)
                            .await
                            .map_err(|error: crate::pipeline::ClosedError| {
                                error!(message = "Error sending to sink.", %error);
                            })?;
                    }
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
) -> Result<(async_nats::Connection, async_nats::Subscription), BuildError> {
    let nc = config.connect().await?;

    let subscription = match &config.queue {
        None => nc.subscribe(&config.subject).await,
        Some(queue) => nc.queue_subscribe(&config.subject, queue).await,
    };

    let subscription = subscription.map_err(|e| BuildError::Subscription { source: e })?;

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
    use crate::test_util::{collect_n, random_string};
    use crate::tls::TlsOptions;
    use std::matches;

    async fn publish_and_check(conf: NatsSourceConfig) -> Result<(), BuildError> {
        let subject = conf.subject.clone();
        let (nc, sub) = create_subscription(&conf).await?;
        let nc_pub = nc.clone();

        let (tx, rx) = Pipeline::new_test();
        let decoder = DecodingConfig::new(conf.framing.clone(), conf.decoding.clone())
            .build()
            .unwrap();
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
        assert!(r.is_ok());
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
            auth: Some(NatsAuthConfig::UserPassword {
                user: "natsuser".into(),
                password: "natspass".into(),
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(r.is_ok());
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
            auth: Some(NatsAuthConfig::UserPassword {
                user: "natsuser".into(),
                password: "wrongpass".into(),
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(matches!(r, Err(BuildError::Connection { .. })));
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
            auth: Some(NatsAuthConfig::Token {
                token: "secret".into(),
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(r.is_ok());
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
            auth: Some(NatsAuthConfig::Token {
                token: "wrongsecret".into(),
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(matches!(r, Err(BuildError::Connection { .. })));
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
            auth: Some(NatsAuthConfig::NKey {
                nkey: "UD345ZYSUJQD7PNCTWQPINYSO3VH4JBSADBSYUZOBT666DRASFRAWAWT".into(),
                seed: "SUANIRXEZUROTXNFN3TJYMT27K7ZZVMD46FRIHF6KXKS4KGNVBS57YAFGY".into(),
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(r.is_ok());
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
            auth: Some(NatsAuthConfig::NKey {
                nkey: "UAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                seed: "SBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into(),
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(matches!(r, Err(BuildError::Config { .. })));
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
        assert!(r.is_ok());
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
        assert!(matches!(r, Err(BuildError::Connection { .. })));
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
        assert!(r.is_ok());
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
        assert!(matches!(r, Err(BuildError::Connection { .. })));
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
            auth: Some(NatsAuthConfig::CredentialsFile {
                path: "tests/data/nats.creds".into(),
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(r.is_ok());
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
            auth: Some(NatsAuthConfig::CredentialsFile {
                path: "tests/data/nats-bad.creds".into(),
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(matches!(r, Err(BuildError::Connection { .. })));
    }
}
