use std::convert::TryFrom;

use async_trait::async_trait;
use bytes::BytesMut;
use codecs::{encoding::SerializerConfig, JsonSerializerConfig, RawMessageSerializerConfig};
use futures::{stream::BoxStream, FutureExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::Encoder as _;
use vector_buffers::Acker;

use crate::{
    codecs::Encoder,
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, SinkDescription,
    },
    event::Event,
    internal_events::{NatsEventSendError, NatsEventSendSuccess, TemplateRenderingError},
    nats::{from_tls_auth_config, NatsAuthConfig, NatsConfigError},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfigAdapter, EncodingConfigMigrator, Transformer},
        StreamSink,
    },
    template::{Template, TemplateParseError},
    tls::TlsEnableableConfig,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("invalid subject template: {}", source))]
    SubjectTemplate { source: TemplateParseError },
    #[snafu(display("NATS Config Error: {}", source))]
    Config { source: NatsConfigError },
    #[snafu(display("NATS Connect Error: {}", source))]
    Connect { source: std::io::Error },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingMigrator;

impl EncodingConfigMigrator for EncodingMigrator {
    type Codec = Encoding;

    fn migrate(codec: &Self::Codec) -> SerializerConfig {
        match codec {
            Encoding::Text => RawMessageSerializerConfig::new().into(),
            Encoding::Json => JsonSerializerConfig::new().into(),
        }
    }
}

/**
 * Code dealing with the SinkConfig struct.
 */

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NatsSinkConfig {
    #[serde(flatten)]
    encoding: EncodingConfigAdapter<EncodingConfig<Encoding>, EncodingMigrator>,
    #[serde(default = "default_name", alias = "name")]
    connection_name: String,
    subject: String,
    url: String,
    tls: Option<TlsEnableableConfig>,
    auth: Option<NatsAuthConfig>,
}

fn default_name() -> String {
    String::from("vector")
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

inventory::submit! {
    SinkDescription::new::<NatsSinkConfig>("nats")
}

impl GenerateConfig for NatsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            encoding.codec = "json"
            connection_name = "vector"
            subject = "from.vector"
            url = "nats://127.0.0.1:4222""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "nats")]
impl SinkConfig for NatsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink = NatsSink::new(self.clone(), cx.acker()).await?;
        let healthcheck = healthcheck(self.clone()).boxed();
        Ok((super::VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type())
    }

    fn sink_type(&self) -> &'static str {
        "nats"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        None
    }
}

impl std::convert::TryFrom<&NatsSinkConfig> for nats::asynk::Options {
    type Error = NatsConfigError;

    fn try_from(config: &NatsSinkConfig) -> Result<Self, Self::Error> {
        from_tls_auth_config(&config.connection_name, &config.auth, &config.tls)
    }
}

impl NatsSinkConfig {
    async fn connect(&self) -> Result<nats::asynk::Connection, BuildError> {
        let options: nats::asynk::Options = self.try_into().context(ConfigSnafu)?;

        options.connect(&self.url).await.context(ConnectSnafu)
    }
}

async fn healthcheck(config: NatsSinkConfig) -> crate::Result<()> {
    config.connect().map_ok(|_| ()).map_err(|e| e.into()).await
}

pub struct NatsSink {
    transformer: Transformer,
    encoder: Encoder<()>,
    connection: nats::asynk::Connection,
    subject: Template,
    acker: Acker,
}

impl NatsSink {
    async fn new(config: NatsSinkConfig, acker: Acker) -> Result<Self, BuildError> {
        let connection = config.connect().await?;
        let encoding = config.encoding.clone();
        let transformer = encoding.transformer();
        let serializer = encoding.encoding();
        let encoder = Encoder::<()>::new(serializer);

        Ok(NatsSink {
            connection,
            transformer,
            encoder,
            subject: Template::try_from(config.subject).context(SubjectTemplateSnafu)?,
            acker,
        })
    }
}

#[async_trait]
impl StreamSink<Event> for NatsSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(mut event) = input.next().await {
            let subject = match self.subject.render_string(&event) {
                Ok(subject) => subject,
                Err(error) => {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("subject"),
                        drop_event: true,
                    });
                    self.acker.ack(1);
                    continue;
                }
            };

            self.transformer.transform(&mut event);

            let mut bytes = BytesMut::new();
            if self.encoder.encode(event, &mut bytes).is_err() {
                // Error is logged by `Encoder`.
                continue;
            }

            let byte_size = bytes.len();

            match self.connection.publish(&subject, bytes).await {
                Ok(_) => {
                    emit!(NatsEventSendSuccess { byte_size });
                }
                Err(error) => {
                    emit!(NatsEventSendError { error });
                }
            }

            self.acker.ack(1);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<NatsSinkConfig>();
    }
}

#[cfg(feature = "nats-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use std::{thread, time::Duration};

    use super::*;
    use crate::nats::{NatsAuthCredentialsFile, NatsAuthNKey, NatsAuthToken, NatsAuthUserPassword};
    use crate::sinks::VectorSink;
    use crate::test_util::{random_lines_with_stream, random_string, trace_init};
    use crate::tls::TlsConfig;

    async fn publish_and_check(conf: NatsSinkConfig) -> Result<(), BuildError> {
        // Publish `N` messages to NATS.
        //
        // Verify with a separate subscriber that the messages were
        // successfully published.

        // Create Sink
        let (acker, ack_counter) = Acker::basic();
        let sink = NatsSink::new(conf.clone(), acker).await?;
        let sink = VectorSink::from_event_streamsink(sink);

        // Establish the consumer subscription.
        let subject = conf.subject.clone();
        let consumer = conf
            .clone()
            .connect()
            .await
            .expect("failed to connect with test consumer");
        let sub = consumer
            .subscribe(&subject)
            .await
            .expect("failed to subscribe with test consumer");

        // Publish events.
        let num_events = 1_000;
        let (input, events) = random_lines_with_stream(100, num_events, None);

        let _ = sink.run(events).await.unwrap();

        // Unsubscribe from the channel.
        thread::sleep(Duration::from_secs(3));
        let _ = sub.drain().await.unwrap();

        let mut output: Vec<String> = Vec::new();
        while let Some(msg) = sub.next().await {
            output.push(String::from_utf8_lossy(&msg.data).to_string())
        }

        assert_eq!(output.len(), input.len());
        assert_eq!(output, input);

        assert_eq!(
            ack_counter.load(std::sync::atomic::Ordering::Relaxed),
            num_events
        );

        Ok(())
    }

    #[tokio::test]
    async fn nats_no_auth() {
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4222".to_owned(),
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4223".to_owned(),
            tls: None,
            auth: Some(NatsAuthConfig::UserPassword {
                user_password: NatsAuthUserPassword {
                    user: "natsuser".into(),
                    password: "natspass".into(),
                },
            }),
        };

        publish_and_check(conf)
            .await
            .expect("publish_and_check failed");
    }

    #[tokio::test]
    async fn nats_userpass_auth_invalid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4224".to_owned(),
            tls: None,
            auth: Some(NatsAuthConfig::UserPassword {
                user_password: NatsAuthUserPassword {
                    user: "natsuser".into(),
                    password: "wrongpass".into(),
                },
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4224".to_owned(),
            tls: None,
            auth: Some(NatsAuthConfig::Token {
                token: NatsAuthToken {
                    value: "secret".into(),
                },
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4224".to_owned(),
            tls: None,
            auth: Some(NatsAuthConfig::Token {
                token: NatsAuthToken {
                    value: "wrongsecret".into(),
                },
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4225".to_owned(),
            tls: None,
            auth: Some(NatsAuthConfig::Nkey {
                nkey: NatsAuthNKey {
                    nkey: "UD345ZYSUJQD7PNCTWQPINYSO3VH4JBSADBSYUZOBT666DRASFRAWAWT".into(),
                    seed: "SUANIRXEZUROTXNFN3TJYMT27K7ZZVMD46FRIHF6KXKS4KGNVBS57YAFGY".into(),
                },
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4225".to_owned(),
            tls: None,
            auth: Some(NatsAuthConfig::Nkey {
                nkey: NatsAuthNKey {
                    nkey: "UAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                    seed: "SBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into(),
                },
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4227".to_owned(),
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4227".to_owned(),
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4228".to_owned(),
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4228".to_owned(),
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4229".to_owned(),
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    ca_file: Some("tests/data/mkcert_rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: Some(NatsAuthConfig::CredentialsFile {
                credentials_file: NatsAuthCredentialsFile {
                    path: "tests/data/nats.creds".into(),
                },
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4229".to_owned(),
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    ca_file: Some("tests/data/mkcert_rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: Some(NatsAuthConfig::CredentialsFile {
                credentials_file: NatsAuthCredentialsFile {
                    path: "tests/data/nats-bad.creds".into(),
                },
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
