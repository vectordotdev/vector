use vector_lib::{
    codecs::decoding::{DeserializerConfig, FramingConfig},
    config::{LegacyKey, LogNamespace, SourceAcknowledgementsConfig},
    configurable::configurable_component,
    lookup::{lookup_v2::OptionalValuePath, owned_value_path},
};
use vrl::value::{Kind, kind::Collection};

use super::source::MqttSource;
use crate::{
    codecs::DecodingConfig,
    common::mqtt::{self, MqttCommonConfig},
    config::{SourceConfig, SourceContext, SourceOutput},
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

    /// Overrides the name of the log field used to add the MQTT protocol version to each event.
    ///
    /// By default, `"protocol_version"` is used.
    #[serde(default = "default_protocol_version_key")]
    #[configurable(metadata(docs::examples = "protocol_version"))]
    pub protocol_version_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the MQTT v5 content type to each event.
    ///
    /// By default, `"content_type"` is used.
    #[serde(default = "default_content_type_key")]
    #[configurable(metadata(docs::examples = "content_type"))]
    pub content_type_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the MQTT v5 response topic to each event.
    ///
    /// By default, `"response_topic"` is used.
    #[serde(default = "default_response_topic_key")]
    #[configurable(metadata(docs::examples = "response_topic"))]
    pub response_topic_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the MQTT v5 correlation data to each event.
    ///
    /// By default, `"correlation_data"` is used.
    #[serde(default = "default_correlation_data_key")]
    #[configurable(metadata(docs::examples = "correlation_data"))]
    pub correlation_data_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the MQTT v5 payload format indicator to each event.
    ///
    /// By default, `"payload_format_indicator"` is used.
    #[serde(default = "default_payload_format_indicator_key")]
    #[configurable(metadata(docs::examples = "payload_format_indicator"))]
    pub payload_format_indicator_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the MQTT v5 message expiry interval to each event.
    ///
    /// By default, `"message_expiry_interval"` is used.
    #[serde(default = "default_message_expiry_interval_key")]
    #[configurable(metadata(docs::examples = "message_expiry_interval"))]
    pub message_expiry_interval_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the MQTT v5 user properties to each event.
    ///
    /// By default, `"user_properties"` is used.
    #[serde(default = "default_user_properties_key")]
    #[configurable(metadata(docs::examples = "user_properties"))]
    pub user_properties_key: OptionalValuePath,

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

fn default_protocol_version_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("protocol_version"))
}

fn default_content_type_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("content_type"))
}

fn default_response_topic_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("response_topic"))
}

fn default_correlation_data_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("correlation_data"))
}

fn default_payload_format_indicator_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("payload_format_indicator"))
}

fn default_message_expiry_interval_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("message_expiry_interval"))
}

fn default_user_properties_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("user_properties"))
}

#[async_trait::async_trait]
#[typetag::serde(name = "mqtt")]
impl SourceConfig for MqttSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        let connector =
            mqtt::build_connector(&self.common, "vectorSource", false, acknowledgements)?;

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
                self.topic_key.path.clone().map(LegacyKey::Overwrite),
                &owned_value_path!("topic"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("timestamp"))),
                &owned_value_path!("timestamp"),
                Kind::timestamp().or_undefined(),
                Some("timestamp"),
            )
            .with_source_metadata(
                Self::NAME,
                self.protocol_version_key
                    .path
                    .clone()
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("protocol_version"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.content_type_key.path.clone().map(LegacyKey::Overwrite),
                &owned_value_path!("content_type"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.response_topic_key
                    .path
                    .clone()
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("response_topic"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.correlation_data_key
                    .path
                    .clone()
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("correlation_data"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.payload_format_indicator_key
                    .path
                    .clone()
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("payload_format_indicator"),
                Kind::integer().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.message_expiry_interval_key
                    .path
                    .clone()
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("message_expiry_interval"),
                Kind::integer().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.user_properties_key
                    .path
                    .clone()
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("user_properties"),
                Kind::array(Collection::from_unknown(Kind::object(
                    std::collections::BTreeMap::from([
                        ("key".into(), Kind::bytes()),
                        ("value".into(), Kind::bytes()),
                    ]),
                )))
                .or_undefined(),
                None,
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

impl_generate_config_from_default!(MqttSourceConfig);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MqttSourceConfig>();
    }
}
