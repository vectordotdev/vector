use std::convert::TryFrom;

use async_trait::async_trait;
use futures::{stream::BoxStream, FutureExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use vector_buffers::Acker;

use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, SinkDescription,
    },
    event::Event,
    internal_events::{NatsEventSendError, NatsEventSendSuccess, TemplateRenderingError},
    nats::{from_tls_auth_config, NatsAuthConfig, NatsConfigError},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        StreamSink,
    },
    template::{Template, TemplateParseError},
    tls::TlsConfig,
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

/**
 * Code dealing with the SinkConfig struct.
 */

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NatsSinkConfig {
    encoding: EncodingConfig<Encoding>,
    #[serde(default = "default_name", alias = "name")]
    connection_name: String,
    subject: String,
    url: String,
    tls: Option<TlsConfig>,
    #[serde(flatten)]
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
        Input::log()
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
    encoding: EncodingConfig<Encoding>,
    connection: nats::asynk::Connection,
    subject: Template,
    acker: Acker,
}

impl NatsSink {
    async fn new(config: NatsSinkConfig, acker: Acker) -> Result<Self, BuildError> {
        let connection = config.connect().await?;

        Ok(NatsSink {
            connection,
            encoding: config.encoding,
            subject: Template::try_from(config.subject).context(SubjectTemplateSnafu)?,
            acker,
        })
    }
}

#[async_trait]
impl StreamSink<Event> for NatsSink {
    async fn run(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(event) = input.next().await {
            let subject = match self.subject.render_string(&event) {
                Ok(subject) => subject,
                Err(error) => {
                    emit!(&TemplateRenderingError {
                        error,
                        field: Some("subject"),
                        drop_event: true,
                    });
                    self.acker.ack(1);
                    continue;
                }
            };

            let log = encode_event(event, &self.encoding);
            let message_len = log.len();

            match self.connection.publish(&subject, log).await {
                Ok(_) => {
                    emit!(&NatsEventSendSuccess {
                        byte_size: message_len,
                    });
                }
                Err(error) => {
                    emit!(&NatsEventSendError { error });
                }
            }

            self.acker.ack(1);
        }

        Ok(())
    }
}

fn encode_event(mut event: Event, encoding: &EncodingConfig<Encoding>) -> String {
    encoding.apply_rules(&mut event);

    match encoding.codec() {
        Encoding::Json => serde_json::to_string(event.as_log()).unwrap(),
        Encoding::Text => event
            .as_log()
            .get(crate::config::log_schema().message_key())
            .map(|v| v.to_string_lossy())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::{encode_event, Encoding, EncodingConfig, *};
    use crate::event::{Event, Value};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<NatsSinkConfig>();
    }

    #[test]
    fn encodes_raw_logs() {
        let event = Event::from("foo");
        assert_eq!(
            "foo",
            encode_event(event, &EncodingConfig::from(Encoding::Text))
        );
    }

    #[test]
    fn encodes_log_events() {
        let mut event = Event::new_empty_log();
        let log = event.as_mut_log();
        log.insert("x", Value::from("23"));
        log.insert("z", Value::from(25));
        log.insert("a", Value::from("0"));

        let encoded = encode_event(event, &EncodingConfig::from(Encoding::Json));
        let expected = r#"{"a":"0","x":"23","z":25}"#;
        assert_eq!(encoded, expected);
    }
}

#[cfg(feature = "nats-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use std::{thread, time::Duration};

    use super::*;
    use crate::nats::{
        NatsAuthCredentialsFile, NatsAuthNKey, NatsAuthStrategy, NatsAuthToken,
        NatsAuthUserPassword,
    };
    use crate::sinks::VectorSink;
    use crate::test_util::{random_lines_with_stream, random_string, trace_init};
    use crate::tls::TlsOptions;

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
            encoding: EncodingConfig::from(Encoding::Text),
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
            encoding: EncodingConfig::from(Encoding::Text),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4223".to_owned(),
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4224".to_owned(),
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4224".to_owned(),
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4224".to_owned(),
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4225".to_owned(),
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4225".to_owned(),
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4227".to_owned(),
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text),
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
            encoding: EncodingConfig::from(Encoding::Text),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4228".to_owned(),
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4228".to_owned(),
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4229".to_owned(),
            tls: Some(TlsConfig {
                enabled: Some(true),
                options: TlsOptions {
                    ca_file: Some("tests/data/mkcert_rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: Some(NatsAuthConfig {
                strategy: NatsAuthStrategy::CredentialsFile,
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
        trace_init();

        let subject = format!("test-{}", random_string(10));

        let conf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://localhost:4229".to_owned(),
            tls: Some(TlsConfig {
                enabled: Some(true),
                options: TlsOptions {
                    ca_file: Some("tests/data/mkcert_rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: Some(NatsAuthConfig {
                strategy: NatsAuthStrategy::CredentialsFile,
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
