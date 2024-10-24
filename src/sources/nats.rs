use chrono::Utc;
use futures::{pin_mut, StreamExt};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::FramedRead;
use vector_lib::codecs::decoding::{DeserializerConfig, FramingConfig, StreamDecodingError};
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, CountByteSize, EventsReceived, InternalEventHandle as _, Protocol,
};
use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path};
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    EstimatedJsonEncodedSizeOf,
};
use vrl::value::Kind;

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{GenerateConfig, SourceConfig, SourceContext, SourceOutput},
    event::Event,
    internal_events::StreamClosedError,
    nats::{from_tls_auth_config, NatsAuthConfig, NatsConfigError},
    serde::{default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    tls::TlsEnableableConfig,
    SourceSender,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("NATS Config Error: {}", source))]
    Config { source: NatsConfigError },
    #[snafu(display("NATS Connect Error: {}", source))]
    Connect { source: async_nats::ConnectError },
    #[snafu(display("NATS Subscribe Error: {}", source))]
    Subscribe { source: async_nats::SubscribeError },
}

/// Configuration for the `nats` source.
#[configurable_component(source(
    "nats",
    "Read observability data from subjects on the NATS messaging system."
))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct NatsSourceConfig {
    /// The NATS URL to connect to.
    ///
    /// The URL takes the form of `nats://server:port`.
    /// If the port is not specified it defaults to 4222.
    #[configurable(metadata(docs::examples = "nats://demo.nats.io"))]
    #[configurable(metadata(docs::examples = "nats://127.0.0.1:4242"))]
    #[configurable(metadata(
        docs::examples = "nats://localhost:4222,nats://localhost:5222,nats://localhost:6222"
    ))]
    url: String,

    /// A [name][nats_connection_name] assigned to the NATS connection.
    ///
    /// [nats_connection_name]: https://docs.nats.io/using-nats/developer/connecting/name
    #[serde(alias = "name")]
    #[configurable(metadata(docs::examples = "vector"))]
    connection_name: String,

    /// The NATS [subject][nats_subject] to pull messages from.
    ///
    /// [nats_subject]: https://docs.nats.io/nats-concepts/subjects
    #[configurable(metadata(docs::examples = "foo"))]
    #[configurable(metadata(docs::examples = "time.us.east"))]
    #[configurable(metadata(docs::examples = "time.*.east"))]
    #[configurable(metadata(docs::examples = "time.>"))]
    #[configurable(metadata(docs::examples = ">"))]
    subject: String,

    /// The NATS queue group to join.
    queue: Option<String>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    auth: Option<NatsAuthConfig>,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    decoding: DeserializerConfig,

    /// The `NATS` subject key.
    #[serde(default = "default_subject_key_field")]
    subject_key_field: OptionalValuePath,

    /// The buffer capacity of the underlying NATS subscriber.
    ///
    /// This value determines how many messages the NATS subscriber buffers
    /// before incoming messages are dropped.
    ///
    /// See the [async_nats documentation][async_nats_subscription_capacity] for more information.
    ///
    /// [async_nats_subscription_capacity]: https://docs.rs/async-nats/latest/async_nats/struct.ConnectOptions.html#method.subscription_capacity
    #[serde(default = "default_subscription_capacity")]
    #[derivative(Default(value = "default_subscription_capacity()"))]
    subscriber_capacity: usize,
}

fn default_subject_key_field() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("subject"))
}

const fn default_subscription_capacity() -> usize {
    65536
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
        let log_namespace = cx.log_namespace(self.log_namespace);
        let (connection, subscription) = create_subscription(self).await?;
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        Ok(Box::pin(nats_source(
            self.clone(),
            connection,
            subscription,
            decoder,
            log_namespace,
            cx.shutdown,
            cx.out,
        )))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let legacy_subject_key_field = self
            .subject_key_field
            .clone()
            .path
            .map(LegacyKey::InsertIfEmpty);
        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                NatsSourceConfig::NAME,
                legacy_subject_key_field,
                &owned_value_path!("subject"),
                Kind::bytes(),
                None,
            );

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

impl NatsSourceConfig {
    async fn connect(&self) -> Result<async_nats::Client, BuildError> {
        let options: async_nats::ConnectOptions = self.try_into().context(ConfigSnafu)?;

        let server_addrs = self.parse_server_addresses()?;
        options.connect(server_addrs).await.context(ConnectSnafu)
    }

    fn parse_server_addresses(&self) -> Result<Vec<async_nats::ServerAddr>, BuildError> {
        self.url
            .split(',')
            .map(|url| {
                url.parse::<async_nats::ServerAddr>()
                    .map_err(|_| BuildError::Connect {
                        source: async_nats::ConnectErrorKind::ServerParse.into(),
                    })
            })
            .collect()
    }
}

impl TryFrom<&NatsSourceConfig> for async_nats::ConnectOptions {
    type Error = NatsConfigError;

    fn try_from(config: &NatsSourceConfig) -> Result<Self, Self::Error> {
        from_tls_auth_config(&config.connection_name, &config.auth, &config.tls)
            .map(|options| options.subscription_capacity(config.subscriber_capacity))
    }
}

async fn nats_source(
    config: NatsSourceConfig,
    // Take ownership of the connection so it doesn't get dropped.
    _connection: async_nats::Client,
    subscriber: async_nats::Subscriber,
    decoder: Decoder,
    log_namespace: LogNamespace,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let events_received = register!(EventsReceived);
    let stream = subscriber.take_until(shutdown);
    pin_mut!(stream);
    let bytes_received = register!(BytesReceived::from(Protocol::TCP));
    while let Some(msg) = stream.next().await {
        bytes_received.emit(ByteSize(msg.payload.len()));
        let mut stream = FramedRead::new(msg.payload.as_ref(), decoder.clone());
        while let Some(next) = stream.next().await {
            match next {
                Ok((events, _byte_size)) => {
                    let count = events.len();
                    let byte_size = events.estimated_json_encoded_size_of();
                    events_received.emit(CountByteSize(count, byte_size));

                    let now = Utc::now();

                    let events = events.into_iter().map(|mut event| {
                        if let Event::Log(ref mut log) = event {
                            log_namespace.insert_standard_vector_source_metadata(
                                log,
                                NatsSourceConfig::NAME,
                                now,
                            );

                            let legacy_subject_key_field = config
                                .subject_key_field
                                .path
                                .as_ref()
                                .map(LegacyKey::InsertIfEmpty);
                            log_namespace.insert_source_metadata(
                                NatsSourceConfig::NAME,
                                log,
                                legacy_subject_key_field,
                                &owned_value_path!("subject"),
                                msg.subject.as_str(),
                            )
                        }
                        event
                    });

                    out.send_batch(events).await.map_err(|_| {
                        emit!(StreamClosedError { count });
                    })?;
                }
                Err(error) => {
                    // Error is logged by `crate::codecs`, no further
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
) -> Result<(async_nats::Client, async_nats::Subscriber), BuildError> {
    let nc = config.connect().await?;

    let subscription = match &config.queue {
        None => nc.subscribe(config.subject.clone()).await,
        Some(queue) => {
            nc.queue_subscribe(config.subject.clone(), queue.clone())
                .await
        }
    };

    let subscription = subscription.context(SubscribeSnafu)?;

    Ok((nc, subscription))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::print_stdout)] //tests

    use vector_lib::lookup::{owned_value_path, OwnedTargetPath};
    use vector_lib::schema::Definition;
    use vrl::value::{kind::Collection, Kind};

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<NatsSourceConfig>();
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = NatsSourceConfig {
            log_namespace: Some(true),
            subject_key_field: default_subject_key_field(),
            ..Default::default()
        };

        let definitions = config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        let expected_definition =
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                .with_meaning(OwnedTargetPath::event_root(), "message")
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None,
                )
                .with_metadata_field(&owned_value_path!("nats", "subject"), Kind::bytes(), None);

        assert_eq!(definitions, Some(expected_definition));
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = NatsSourceConfig {
            subject_key_field: default_subject_key_field(),
            ..Default::default()
        };
        let definitions = config
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        let expected_definition = Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Legacy],
        )
        .with_event_field(
            &owned_value_path!("message"),
            Kind::bytes(),
            Some("message"),
        )
        .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
        .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
        .with_event_field(&owned_value_path!("subject"), Kind::bytes(), None);

        assert_eq!(definitions, Some(expected_definition));
    }
}

#[cfg(feature = "nats-integration-tests")]
#[cfg(test)]
mod integration_tests {
    #![allow(clippy::print_stdout)] //tests

    use bytes::Bytes;
    use vector_lib::config::log_schema;

    use super::*;
    use crate::nats::{NatsAuthCredentialsFile, NatsAuthNKey, NatsAuthToken, NatsAuthUserPassword};
    use crate::test_util::{
        collect_n,
        components::{assert_source_compliance, SOURCE_TAGS},
        random_string,
    };
    use crate::tls::TlsConfig;

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
            tokio::spawn(nats_source(
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

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: None,
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
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
        let url = std::env::var("NATS_USERPASS_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: Some(NatsAuthConfig::UserPassword {
                user_password: NatsAuthUserPassword {
                    user: "natsuser".to_string(),
                    password: "natspass".to_string().into(),
                },
            }),
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
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
        let url = std::env::var("NATS_USERPASS_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: Some(NatsAuthConfig::UserPassword {
                user_password: NatsAuthUserPassword {
                    user: "natsuser".to_string(),
                    password: "wrongpass".to_string().into(),
                },
            }),
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
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
        let url = std::env::var("NATS_TOKEN_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: Some(NatsAuthConfig::Token {
                token: NatsAuthToken {
                    value: "secret".to_string().into(),
                },
            }),
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
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
        let url = std::env::var("NATS_TOKEN_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: Some(NatsAuthConfig::Token {
                token: NatsAuthToken {
                    value: "wrongsecret".to_string().into(),
                },
            }),
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
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
        let url = std::env::var("NATS_NKEY_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: Some(NatsAuthConfig::Nkey {
                nkey: NatsAuthNKey {
                    nkey: "UD345ZYSUJQD7PNCTWQPINYSO3VH4JBSADBSYUZOBT666DRASFRAWAWT".into(),
                    seed: "SUANIRXEZUROTXNFN3TJYMT27K7ZZVMD46FRIHF6KXKS4KGNVBS57YAFGY".into(),
                },
            }),
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
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
        let url = std::env::var("NATS_NKEY_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: Some(NatsAuthConfig::Nkey {
                nkey: NatsAuthNKey {
                    nkey: "UAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                    seed: "SBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into(),
                },
            }),
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Config, got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_tls_valid() {
        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TLS_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    ca_file: Some("tests/data/nats/rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: None,
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
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
        let url = std::env::var("NATS_TLS_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: None,
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
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
        let url = std::env::var("NATS_TLS_CLIENT_CERT_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
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
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
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
        let url = std::env::var("NATS_TLS_CLIENT_CERT_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    ca_file: Some("tests/data/nats/rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: None,
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
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
        let url = std::env::var("NATS_JWT_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
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
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
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
        let url = std::env::var("NATS_JWT_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
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
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_multiple_urls_valid() {
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://nats:4222,nats://demo.nats.io:4222".to_string(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: None,
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
        };

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed for multiple URLs, expected Ok(()), got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_multiple_urls_invalid() {
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "http://invalid-url,nats://:invalid@localhost:4222".to_string(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            tls: None,
            auth: None,
            log_namespace: None,
            subject_key_field: default_subject_key_field(),
            ..Default::default()
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed for bad URLs, expected BuildError::Connect, got: {:?}",
            r
        );
    }
}
