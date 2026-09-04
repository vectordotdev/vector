use async_nats::{HeaderMap, header};
use bytes::Bytes;
use futures_util::TryFutureExt;
use snafu::ResultExt;
use vector_lib::{codecs::JsonSerializerConfig, tls::TlsEnableableConfig};

use super::{ConfigSnafu, ConnectSnafu, NatsError, sink::NatsSink};
use crate::{
    config::ValidatedSink,
    nats::{NatsAuthConfig, NatsConfigError, from_tls_auth_config, validate_tls_cert_key_pair},
    sinks::{prelude::*, util::service::TowerRequestConfigDefaults},
    template::ConfinementConfig,
};

#[derive(Clone, Copy, Debug)]
pub struct NatsTowerRequestConfigDefaults;

impl TowerRequestConfigDefaults for NatsTowerRequestConfigDefaults {
    const CONCURRENCY: Concurrency = Concurrency::None;
}

/// A set of NATS headers that can be added to each message.
#[configurable_component]
#[serde_with::serde_as]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NatsHeaderConfig {
    /// A unique identifier for the message. Useful for deduplication.
    ///
    /// Can be a template that references fields in the event, e.g., `{{ event_id }}`.
    #[configurable(metadata(docs::templateable))]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[configurable(metadata(docs::examples = "event-{{ event_id }}"))]
    pub(super) message_id: Option<UnconfinedTemplate>,
}

impl NatsHeaderConfig {
    pub fn build_headers(&self, event: &Event) -> HeaderMap {
        let mut headers = HeaderMap::new();

        if let Some(template) = &self.message_id
            && let Ok(value) = template.render_string(event)
        {
            headers.insert(header::NATS_MESSAGE_ID, value.as_str());
        }

        headers
    }
}

/// Configuration for sending messages using NATS JetStream.
#[configurable_component]
#[serde_with::serde_as]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct JetStreamConfig {
    /// Whether to enable Jetstream.
    #[serde(default)]
    pub enabled: bool,

    /// A map of NATS headers to be included in each message.
    #[configurable(metadata(docs::templateable))]
    #[serde(default)]
    pub(super) headers: Option<NatsHeaderConfig>,
}

impl From<bool> for JetStreamConfig {
    fn from(enabled: bool) -> Self {
        Self {
            enabled,
            ..Default::default()
        }
    }
}

/// Configuration for the `nats` sink.
#[configurable_component(sink(
    "nats",
    "Publish observability data to subjects on the NATS messaging system."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct NatsSinkConfig {
    pub(super) encoding: EncodingConfig,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    /// A NATS [name][nats_connection_name] assigned to the NATS connection.
    ///
    /// [nats_connection_name]: https://docs.nats.io/using-nats/developer/connecting/name
    #[serde(default = "default_name", alias = "name")]
    #[configurable(metadata(docs::examples = "foo"))]
    pub(super) connection_name: String,

    /// The NATS [subject][nats_subject] to publish messages to.
    ///
    /// [nats_subject]: https://docs.nats.io/nats-concepts/subjects
    #[configurable(metadata(docs::templateable))]
    #[configurable(metadata(
        docs::examples = "events-{{ host }}",
        docs::examples = "foo",
        docs::examples = "time.us.east",
        docs::examples = "time.*.east",
        docs::examples = "time.>",
        docs::examples = ">"
    ))]
    pub(super) subject: Template,

    /// The NATS [URL][nats_url] to connect to.
    ///
    /// The URL must take the form of `nats://server:port`.
    /// If the port is not specified it defaults to 4222.
    ///
    /// [nats_url]: https://docs.nats.io/using-nats/developer/connecting#nats-url
    #[configurable(metadata(docs::examples = "nats://demo.nats.io"))]
    #[configurable(metadata(docs::examples = "nats://127.0.0.1:4242"))]
    #[configurable(metadata(
        docs::examples = "nats://localhost:4222,nats://localhost:5222,nats://localhost:6222"
    ))]
    pub(super) url: String,

    pub(super) tls: Option<TlsEnableableConfig>,

    pub(super) auth: Option<NatsAuthConfig>,

    #[serde(default)]
    pub(super) request: TowerRequestConfig<NatsTowerRequestConfigDefaults>,

    /// Send messages using [Jetstream][jetstream].
    ///
    /// If set, the `subject` must belong to an existing JetStream stream.
    ///
    /// [jetstream]: https://docs.nats.io/nats-concepts/jetstream
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub(super) jetstream: JetStreamConfig,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,
}

fn default_name() -> String {
    String::from("vector")
}

impl GenerateConfig for NatsSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            acknowledgements: Default::default(),
            auth: None,
            connection_name: "vector".into(),
            encoding: JsonSerializerConfig::default().into(),
            subject: Template::try_from("from.vector").unwrap(),
            tls: None,
            url: "nats://127.0.0.1:4222".into(),
            request: Default::default(),
            jetstream: JetStreamConfig {
                enabled: true,
                headers: Some(NatsHeaderConfig {
                    message_id: Some(UnconfinedTemplate::try_from("event-{{ event_id }}").unwrap()),
                }),
            },
            confinement: ConfinementConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "nats")]
impl SinkConfig for NatsSinkConfig {
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedNatsSink {
    subject: ConfinedTemplate,
    server_addresses: Vec<async_nats::ServerAddr>,
}

#[async_trait::async_trait]
impl ValidatedSink for NatsSinkConfig {
    type Validated = ValidatedNatsSink;

    fn validate(&self) -> crate::Result<ValidatedNatsSink> {
        let subject = self
            .subject
            .clone()
            .confine(&self.confinement, Self::NAME, "subject")?;
        let server_addresses = self.parse_server_addresses()?;

        if let Some(tls) = &self.tls {
            validate_tls_cert_key_pair(tls).context(ConfigSnafu)?;
        }

        Ok(ValidatedNatsSink {
            subject,
            server_addresses,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedNatsSink,
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedNatsSink {
            subject,
            server_addresses,
        } = validated.clone();
        let sink = NatsSink::new(self.clone(), subject, server_addresses.clone()).await?;
        let healthcheck = healthcheck(self.clone(), server_addresses).boxed();
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

impl std::convert::TryFrom<&NatsSinkConfig> for async_nats::ConnectOptions {
    type Error = NatsConfigError;

    fn try_from(config: &NatsSinkConfig) -> Result<Self, Self::Error> {
        from_tls_auth_config(&config.connection_name, &config.auth, &config.tls)
    }
}

impl NatsSinkConfig {
    pub(super) async fn connect(
        &self,
        options: async_nats::ConnectOptions,
        server_addresses: Vec<async_nats::ServerAddr>,
    ) -> Result<async_nats::Client, NatsError> {
        options
            .connect(server_addresses)
            .await
            .context(ConnectSnafu)
    }

    pub(super) fn parse_server_addresses(&self) -> Result<Vec<async_nats::ServerAddr>, NatsError> {
        self.url
            .split(',')
            .map(|url| {
                url.parse::<async_nats::ServerAddr>()
                    .map_err(|_| NatsError::Connect {
                        source: async_nats::ConnectErrorKind::ServerParse.into(),
                    })
            })
            .collect()
    }

    #[cfg(not(test))]
    fn create_connect_options(&self) -> Result<async_nats::ConnectOptions, NatsError> {
        let mut options: async_nats::ConnectOptions = self.try_into().context(ConfigSnafu)?;
        options = options.retry_on_initial_connect();
        Ok(options)
    }

    #[cfg(test)]
    fn create_connect_options(&self) -> Result<async_nats::ConnectOptions, NatsError> {
        let options: async_nats::ConnectOptions = self.try_into().context(ConfigSnafu)?;
        Ok(options)
    }

    pub(super) async fn publisher(
        &self,
        server_addresses: Vec<async_nats::ServerAddr>,
    ) -> Result<NatsPublisher, NatsError> {
        let options = self.create_connect_options()?;
        let connection = self.connect(options, server_addresses).await?;

        if self.jetstream.enabled {
            Ok(NatsPublisher::JetStream(async_nats::jetstream::new(
                connection,
            )))
        } else {
            Ok(NatsPublisher::Core(connection))
        }
    }
}

async fn healthcheck(
    config: NatsSinkConfig,
    server_addresses: Vec<async_nats::ServerAddr>,
) -> crate::Result<()> {
    let options: async_nats::ConnectOptions = (&config).try_into().context(ConfigSnafu)?;
    config
        .connect(options, server_addresses)
        .map_ok(|_| ())
        .map_err(|e| e.into())
        .await
}

pub enum NatsPublisher {
    Core(async_nats::Client),
    JetStream(async_nats::jetstream::Context),
}

impl NatsPublisher {
    pub(super) async fn publish<S: async_nats::subject::ToSubject>(
        &self,
        subject: S,
        headers: HeaderMap,
        payload: Bytes,
    ) -> Result<(), NatsError> {
        match self {
            NatsPublisher::Core(client) => {
                client
                    .publish(subject, payload)
                    .await
                    .map_err(|e| NatsError::PublishError {
                        source: Box::new(e),
                    })?;
                client
                    .flush()
                    .map_ok(|_| ())
                    .map_err(|e| NatsError::PublishError {
                        source: Box::new(e),
                    })
                    .await
            }
            NatsPublisher::JetStream(jetstream) => {
                let ack = jetstream
                    .publish_with_headers(subject, headers, payload)
                    .await
                    .map_err(|e| NatsError::PublishError {
                        source: Box::new(e),
                    })?;
                ack.await.map(|_| ()).map_err(|e| NatsError::PublishError {
                    source: Box::new(e),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ValidatedSink;
    use crate::template::{ConfinementConfig, Template};

    #[test]
    fn validate_rejects_malformed_url() {
        let config: NatsSinkConfig = serde_yaml::from_str(
            r#"
            subject: "test-subject"
            url: "not a valid url"
            encoding:
                codec: "json"
            "#,
        )
        .unwrap();
        assert!(
            config.validate().is_err(),
            "a malformed url should fail validation"
        );
    }

    #[test]
    fn validate_accepts_comma_separated_urls() {
        let config: NatsSinkConfig = serde_yaml::from_str(
            r#"
            subject: "test-subject"
            url: "nats://localhost:4222,nats://localhost:5222"
            encoding:
                codec: "json"
            "#,
        )
        .unwrap();
        assert!(
            config.validate().is_ok(),
            "a comma-separated list of valid urls should pass validation"
        );
    }

    #[test]
    fn validate_returns_confined_subject() {
        let config: NatsSinkConfig = serde_yaml::from_str(
            r#"
            subject: "test-subject"
            url: "nats://127.0.0.1:4222"
            encoding:
                codec: "json"
            "#,
        )
        .unwrap();
        let validated = config.validate().expect("validation should succeed");
        assert_eq!(validated.subject.to_string(), "test-subject");
    }

    #[test]
    fn validate_rejects_tls_cert_without_key() {
        let config: NatsSinkConfig = serde_yaml::from_str(
            r#"
            subject: "test-subject"
            url: "nats://127.0.0.1:4222"
            encoding:
                codec: "json"
            tls:
                enabled: true
                crt_file: "/path/to/crt.pem"
            "#,
        )
        .unwrap();
        assert!(
            config.validate().is_err(),
            "a TLS cert without a key should fail validation"
        );
    }

    #[test]
    fn validate_rejects_tls_key_without_cert() {
        let config: NatsSinkConfig = serde_yaml::from_str(
            r#"
            subject: "test-subject"
            url: "nats://127.0.0.1:4222"
            encoding:
                codec: "json"
            tls:
                enabled: true
                key_file: "/path/to/key.pem"
            "#,
        )
        .unwrap();
        assert!(
            config.validate().is_err(),
            "a TLS key without a cert should fail validation"
        );
    }

    #[test]
    fn validate_accepts_tls_cert_and_key() {
        let config: NatsSinkConfig = serde_yaml::from_str(
            r#"
            subject: "test-subject"
            url: "nats://127.0.0.1:4222"
            encoding:
                codec: "json"
            tls:
                enabled: true
                crt_file: "/path/to/crt.pem"
                key_file: "/path/to/key.pem"
            "#,
        )
        .unwrap();
        assert!(
            config.validate().is_ok(),
            "a complete TLS cert/key pair should pass validation"
        );
    }

    #[test]
    fn validate_accepts_disabled_tls_with_partial_config() {
        let config: NatsSinkConfig = serde_yaml::from_str(
            r#"
            subject: "test-subject"
            url: "nats://127.0.0.1:4222"
            encoding:
                codec: "json"
            tls:
                enabled: false
                crt_file: "/path/to/crt.pem"
            "#,
        )
        .unwrap();
        assert!(
            config.validate().is_ok(),
            "disabled TLS should not fail validation on a lone cert"
        );
    }

    #[test]
    fn confinement_rejects_unconfined_subject() {
        let template = Template::try_from("{{ subject }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "nats", "subject");
        assert!(result.is_err());
    }

    #[test]
    fn confinement_opt_out_allows_unconfined_subject() {
        let template = Template::try_from("{{ subject }}").unwrap();
        let config = ConfinementConfig {
            dangerously_allow_unconfined_template_resolution: true,
        };
        let result = template.confine(&config, "nats", "subject");
        assert!(result.is_ok());
    }

    #[test]
    fn confinement_allows_prefixed_subject() {
        let template = Template::try_from("events-{{ env }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "nats", "subject");
        assert!(result.is_ok());
    }
}
