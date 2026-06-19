use std::time::Duration;

use rand::Rng;
use rumqttc::{MqttOptions, TlsConfiguration, Transport};
use snafu::ResultExt;
use vector_lib::{
    codecs::decoding::{DeserializerConfig, FramingConfig},
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::{lookup_v2::OptionalValuePath, owned_value_path},
    tls::MaybeTlsSettings,
};
use vrl::value::Kind;

use super::source::MqttSource;
use crate::{
    codecs::DecodingConfig,
    common::mqtt::{
        ConfigurationError, ConfigurationSnafu, MqttCommonConfig, MqttConnector, MqttError,
        TlsSnafu,
    },
    config::{SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput},
    serde::{OneOrMany, bool_or_struct, default_decoding, default_framing_message_based},
};

/// Configuration for the `mqtt` source.
#[configurable_component(source("mqtt", "Collect logs from MQTT."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct MqttSourceConfig {
    #[serde(flatten)]
    pub common: MqttCommonConfig,

    /// MQTT topic or topics from which messages are to be read.
    #[configurable(derived)]
    #[serde(default = "default_topic")]
    #[derivative(Default(value = "default_topic()"))]
    pub topic: OneOrMany<String>,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,

    /// Overrides the name of the log field used to add the topic to each event.
    ///
    /// The value is the topic from which the MQTT message was published to.
    ///
    /// By default, `"topic"` is used.
    #[serde(default = "default_topic_key")]
    #[configurable(metadata(docs::examples = "topic"))]
    pub topic_key: OptionalValuePath,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    pub acknowledgements: SourceAcknowledgementsConfig,
}

fn default_topic() -> OneOrMany<String> {
    OneOrMany::One("vector".into())
}

fn default_topic_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("topic"))
}

#[async_trait::async_trait]
#[typetag::serde(name = "mqtt")]
impl SourceConfig for MqttSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        let connector = self.build_connector(acknowledgements)?;

        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let source = MqttSource::new(
            connector.clone(),
            decoder,
            log_namespace,
            self.clone(),
            acknowledgements,
        )?;
        Ok(Box::pin(source.run(cx.out, cx.shutdown)))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let schema_definition = self
            .decoding
            .schema_definition(global_log_namespace.merge(self.log_namespace))
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("timestamp"))),
                &owned_value_path!("timestamp"),
                Kind::timestamp().or_undefined(),
                Some("timestamp"),
            );

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

impl MqttSourceConfig {
    fn build_connector(&self, acknowledgements: bool) -> Result<MqttConnector, MqttError> {
        // End-to-end acknowledgements rely on resuming the MQTT session (and its
        // unacknowledged in-flight messages) after a restart, which is keyed by the
        // client ID. A generated/random client ID would start a fresh session and
        // orphan those messages, silently breaking at-least-once — so require an
        // explicit, stable client ID when acknowledgements are enabled.
        if acknowledgements && self.common.client_id.is_none() {
            return Err(ConfigurationError::AcknowledgementsRequireClientId)
                .context(ConfigurationSnafu);
        }

        let client_id = self.common.client_id.clone().unwrap_or_else(|| {
            let hash = rand::rng()
                .sample_iter(&rand_distr::Alphanumeric)
                .take(6)
                .map(char::from)
                .collect::<String>();
            format!("vectorSource{hash}")
        });

        if client_id.is_empty() {
            return Err(ConfigurationError::InvalidClientId).context(ConfigurationSnafu);
        }

        let tls =
            MaybeTlsSettings::from_config(self.common.tls.as_ref(), false).context(TlsSnafu)?;
        let mut options = MqttOptions::new(client_id, &self.common.host, self.common.port);
        options.set_keep_alive(Duration::from_secs(self.common.keep_alive.into()));
        options.set_max_packet_size(self.common.max_packet_size, self.common.max_packet_size);

        options.set_clean_session(false);

        // With end-to-end acknowledgements enabled, defer the QoS-1 PUBACK until
        // the event has been delivered to all sinks. rumqttc then requires every
        // incoming publish to be acked explicitly via `client.ack(&publish)`.
        // Combined with `clean_session(false)` and QoS `AtLeastOnce`, an unacked
        // message is redelivered by the broker after a crash/reconnect.
        if acknowledgements {
            options.set_manual_acks(true);
        }

        match (&self.common.user, &self.common.password) {
            (Some(user), Some(password)) => {
                options.set_credentials(user, password);
            }
            (None, None) => {
                // Credentials were not provided
            }
            _ => {
                // We need either both username and password, or neither. MQTT also allows for providing only password, but rumqttc does not allow that so we cannot either.
                return Err(ConfigurationError::IncompleteCredentials).context(ConfigurationSnafu);
            }
        }

        if let Some(tls) = tls.tls() {
            let ca = tls.authorities_pem().flatten().collect();
            let client_auth = tls.identity_pem();
            let alpn = Some(vec!["mqtt".into()]);
            options.set_transport(Transport::Tls(TlsConfiguration::Simple {
                ca,
                client_auth,
                alpn,
            }));
        }

        Ok(MqttConnector::new(options))
    }
}

impl_generate_config_from_default!(MqttSourceConfig);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MqttSourceConfig>();
    }

    #[test]
    fn acknowledgements_require_a_stable_client_id() {
        // Without acks, a client ID is auto-generated — fine.
        let default_config = MqttSourceConfig::default();
        assert!(default_config.build_connector(false).is_ok());

        // With acks and no explicit client ID, building must fail (a generated ID
        // would orphan the session's unacknowledged messages after a restart).
        assert!(default_config.build_connector(true).is_err());

        // With acks and an explicit client ID, building succeeds.
        let with_client_id = MqttSourceConfig {
            common: MqttCommonConfig {
                client_id: Some("stable-id".to_owned()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(with_client_id.build_connector(true).is_ok());
    }
}
