use rumqttc::QoS;
use vector_lib::codecs::JsonSerializerConfig;

use crate::{
    codecs::EncodingConfig,
    common::mqtt::{self, MqttCommonConfig, MqttPublishProperties},
    config::{AcknowledgementsConfig, DataType, Input, SinkConfig, SinkContext},
    sinks::{Healthcheck, VectorSink, mqtt::sink::MqttSink, prelude::*},
    template::Template,
};

/// Configuration for the `mqtt` sink
#[configurable_component(sink("mqtt"))]
#[derive(Clone, Debug)]
pub struct MqttSinkConfig {
    #[serde(flatten)]
    pub common: MqttCommonConfig,

    /// If set to true, the MQTT session is cleaned on login.
    #[serde(default = "default_clean_session")]
    pub clean_session: bool,

    /// MQTT publish topic (templates allowed)
    pub topic: Template,

    /// Whether the messages should be retained by the server
    #[serde(default = "default_retain")]
    pub retain: bool,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default = "default_qos")]
    pub quality_of_service: MqttQoS,

    /// MQTT v5 publish properties. Only used when protocol_version is v5.
    #[configurable(derived)]
    #[serde(default)]
    pub publish_properties: Option<MqttPublishProperties>,
}

/// Supported Quality of Service types for MQTT.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(rename_all = "lowercase")]
#[allow(clippy::enum_variant_names)]
pub enum MqttQoS {
    /// AtLeastOnce.
    #[default]
    AtLeastOnce,

    /// AtMostOnce.
    AtMostOnce,

    /// ExactlyOnce.
    ExactlyOnce,
}

impl From<MqttQoS> for QoS {
    fn from(value: MqttQoS) -> Self {
        match value {
            MqttQoS::AtLeastOnce => QoS::AtLeastOnce,
            MqttQoS::AtMostOnce => QoS::AtMostOnce,
            MqttQoS::ExactlyOnce => QoS::ExactlyOnce,
        }
    }
}

impl From<MqttQoS> for rumqttc::v5::mqttbytes::QoS {
    fn from(value: MqttQoS) -> Self {
        match value {
            MqttQoS::AtLeastOnce => rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            MqttQoS::AtMostOnce => rumqttc::v5::mqttbytes::QoS::AtMostOnce,
            MqttQoS::ExactlyOnce => rumqttc::v5::mqttbytes::QoS::ExactlyOnce,
        }
    }
}

const fn default_clean_session() -> bool {
    false
}

const fn default_qos() -> MqttQoS {
    MqttQoS::AtLeastOnce
}

const fn default_retain() -> bool {
    false
}

impl Default for MqttSinkConfig {
    fn default() -> Self {
        Self {
            common: MqttCommonConfig::default(),
            clean_session: default_clean_session(),
            topic: Template::try_from("vector").expect("Cannot parse as a template"),
            retain: default_retain(),
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: AcknowledgementsConfig::default(),
            quality_of_service: MqttQoS::default(),
            publish_properties: None,
        }
    }
}

impl_generate_config_from_default!(MqttSinkConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "mqtt")]
impl SinkConfig for MqttSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let connector = mqtt::build_connector(&self.common, "vectorSink", self.clean_session)?;
        let sink = MqttSink::new(self, connector.clone())?;

        Ok((
            VectorSink::from_event_streamsink(sink),
            Box::pin(async move { connector.healthcheck().await }),
        ))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Log | DataType::Metric | DataType::Trace)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MqttSinkConfig>();
    }
}
