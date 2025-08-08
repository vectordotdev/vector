use std::time::Duration;

use rand::Rng;
use rumqttc::{MqttOptions, QoS, TlsConfiguration, Transport};
use snafu::ResultExt;
use vector_lib::codecs::JsonSerializerConfig;

use crate::template::Template;
use crate::{
    codecs::EncodingConfig,
    common::mqtt::{
        ConfigurationError, ConfigurationSnafu, MqttCommonConfig, MqttConnector, MqttError,
        TlsSnafu,
    },
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    sinks::{mqtt::sink::MqttSink, prelude::*, Healthcheck, VectorSink},
    tls::MaybeTlsSettings,
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
}

/// Supported Quality of Service types for MQTT.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
#[allow(clippy::enum_variant_names)]
pub enum MqttQoS {
    /// AtLeastOnce.
    #[derivative(Default)]
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
        }
    }
}

impl_generate_config_from_default!(MqttSinkConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "mqtt")]
impl SinkConfig for MqttSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let connector = self.build_connector()?;
        let sink = MqttSink::new(self, connector.clone())?;

        Ok((
            VectorSink::from_event_streamsink(sink),
            Box::pin(async move { connector.healthcheck().await }),
        ))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl MqttSinkConfig {
    fn build_connector(&self) -> Result<MqttConnector, MqttError> {
        let client_id = self.common.client_id.clone().unwrap_or_else(|| {
            let hash = rand::rng()
                .sample_iter(&rand_distr::Alphanumeric)
                .take(6)
                .map(char::from)
                .collect::<String>();
            format!("vectorSink{hash}")
        });

        if client_id.is_empty() {
            return Err(ConfigurationError::EmptyClientId).context(ConfigurationSnafu);
        }
        let tls =
            MaybeTlsSettings::from_config(self.common.tls.as_ref(), false).context(TlsSnafu)?;
        let mut options = MqttOptions::new(&client_id, &self.common.host, self.common.port);
        options.set_keep_alive(Duration::from_secs(self.common.keep_alive.into()));
        options.set_max_packet_size(self.common.max_packet_size, self.common.max_packet_size);
        options.set_clean_session(self.clean_session);
        match (&self.common.user, &self.common.password) {
            (Some(user), Some(password)) => {
                options.set_credentials(user, password);
            }
            (None, None) => {}
            _ => {
                return Err(MqttError::Configuration {
                    source: ConfigurationError::InvalidCredentials,
                });
            }
        }
        if let Some(tls) = tls.tls() {
            let ca = tls.authorities_pem().flatten().collect();
            let client_auth = None;
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MqttSinkConfig>();
    }
}
