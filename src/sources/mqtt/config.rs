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

    /// Controls how acknowledgements are handled for this source.
    ///
    /// Prefer enabling `acknowledgements` at the [global][global_acks] or
    /// sink level instead of here: this setting takes precedence over both
    /// when explicitly set, which can silently disable acknowledgements a
    /// connected sink otherwise requires.
    ///
    /// When enabled (through this setting, the global setting, or because a
    /// connected sink requires it), the QoS 1/2 acknowledgement for an
    /// incoming publish is deferred until the resulting events have been
    /// delivered to all connected sinks, giving at-least-once delivery. A
    /// stable `client_id` must also be configured, however acknowledgements
    /// end up enabled, so the MQTT session (and its unacknowledged messages)
    /// can be resumed after a restart.
    ///
    /// [global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
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

        // The ack machinery's liveness (detecting dead connections so unacked
        // messages get redelivered, and its retry timers making progress on a
        // quiet topic) depends on keep-alive traffic; `keep_alive = 0`
        // disables it entirely in rumqttc.
        if acknowledgements && self.common.keep_alive == 0 {
            return Err(ConfigurationError::AcknowledgementsRequireKeepAlive)
                .context(ConfigurationSnafu);
        }

        // An invalid filter is a permanent configuration error: rumqttc
        // queues the SUBSCRIBE without validating it, so it would otherwise
        // only surface at runtime as the broker rejecting (or dropping) the
        // subscription over and over -- a running source that receives
        // nothing. Fail here instead.
        if let Some(topic) = self
            .topic
            .clone()
            .to_vec()
            .into_iter()
            .find(|topic| !rumqttc::valid_filter(topic))
        {
            return Err(ConfigurationError::InvalidTopicFilter { topic }).context(ConfigurationSnafu);
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
            // Honor the user-configured `tls.alpn_protocols` (e.g. `x-amzn-mqtt-ca`, required to
            // reach AWS IoT Core over port 443), falling back to `mqtt` when it is not set.
            let alpn = self
                .common
                .tls
                .as_ref()
                .and_then(|tls| tls.options.alpn_protocols.as_ref())
                .filter(|protocols| !protocols.is_empty())
                .map(|protocols| protocols.iter().map(|p| p.clone().into_bytes()).collect())
                .unwrap_or_else(|| vec![b"mqtt".to_vec()]);
            options.set_transport(Transport::Tls(TlsConfiguration::Simple {
                ca,
                client_auth,
                alpn: Some(alpn),
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

    // With keep-alive disabled a silently dead connection is never detected
    // on a quiet topic, so unacknowledged messages would never be redelivered
    // and the ack machinery's retry timers would never make progress.
    #[test]
    fn acknowledgements_require_keep_alive() {
        let config = MqttSourceConfig {
            common: MqttCommonConfig {
                client_id: Some("stable-id".to_owned()),
                keep_alive: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(config.build_connector(true).is_err());
        // Without acknowledgements, disabling keep-alive stays allowed.
        assert!(config.build_connector(false).is_ok());
    }

    // An invalid topic filter is a permanent configuration error and must
    // fail at build time: rumqttc queues the SUBSCRIBE without validating it,
    // so it would otherwise only surface as a running source that receives
    // nothing while the broker rejects the subscription over and over.
    #[test]
    fn invalid_topic_filters_fail_at_build() {
        let config_with_topic = |topic: OneOrMany<String>| MqttSourceConfig {
            topic,
            ..Default::default()
        };

        for invalid in ["foo/#/bar", "foo/bar#", "foo/b+r", ""] {
            assert!(
                config_with_topic(OneOrMany::One(invalid.into()))
                    .build_connector(false)
                    .is_err(),
                "{invalid:?} must be rejected"
            );
        }

        // One invalid filter among valid ones still fails.
        assert!(
            config_with_topic(OneOrMany::Many(vec!["ok/#".into(), "foo/#/bar".into()]))
                .build_connector(false)
                .is_err()
        );

        for valid in ["foo/#", "foo/+/bar", "+/tele/#", "plain/topic"] {
            assert!(
                config_with_topic(OneOrMany::One(valid.into()))
                    .build_connector(false)
                    .is_ok(),
                "{valid:?} must be accepted"
            );
        }
    }
}
