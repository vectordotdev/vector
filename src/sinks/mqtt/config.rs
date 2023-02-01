use std::time::Duration;

use codecs::JsonSerializerConfig;
use rumqttc::{MqttOptions, QoS, TlsConfiguration, Transport};
use snafu::{ResultExt, Snafu};
use vector_config::configurable_component;

use crate::template::Template;
use crate::{
    codecs::EncodingConfig,
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    sinks::{
        mqtt::sink::{ConfigurationSnafu, MqttConnector, MqttError, MqttSink, TlsSnafu},
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

/// Configuration for the `mqtt` sink
#[configurable_component(sink("mqtt"))]
#[derive(Clone, Debug)]
pub struct MqttSinkConfig {
    /// MQTT server address (The broker’s domain name or IP address).
    #[configurable(metadata(docs::examples = "mqtt.example.com", docs::examples = "127.0.0.1"))]
    pub host: String,

    /// TCP port of the MQTT server to connect to.
    #[serde(default = "default_port")]
    pub port: u16,

    /// MQTT username.
    pub user: Option<String>,

    /// MQTT password.
    pub password: Option<String>,

    /// MQTT client ID.
    #[serde(default = "default_client_id")]
    pub client_id: String,

    /// Connection keep-alive interval.
    #[serde(default = "default_keep_alive")]
    pub keep_alive: u16,

    /// If set to true, the MQTT session is cleaned on login.
    #[serde(default = "default_clean_session")]
    pub clean_session: bool,

    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    /// MQTT publish topic (templates allowed)
    pub topic: Template,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
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
    AtLeastOnce,

    /// AtMostOnce.
    AtMostOnce,

    /// ExactlyOnce.
    #[derivative(Default)]
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

const fn default_port() -> u16 {
    1883
}

fn default_client_id() -> String {
    "vector".to_string()
}

const fn default_keep_alive() -> u16 {
    60
}

const fn default_clean_session() -> bool {
    false
}

impl Default for MqttSinkConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: default_port(),
            user: None,
            password: None,
            client_id: default_client_id(),
            keep_alive: default_keep_alive(),
            clean_session: default_clean_session(),
            tls: None,
            topic: Template::try_from("vector").expect("Cannot parse as a template"),
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: AcknowledgementsConfig::default(),
            quality_of_service: Default::default(),
        }
    }
}

impl_generate_config_from_default!(MqttSinkConfig);

#[async_trait::async_trait]
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

#[derive(Clone, Debug, Eq, PartialEq, Snafu)]
pub enum ConfigurationError {
    #[snafu(display("Client id is not allowed to be empty"))]
    EmptyClientId,
}

impl MqttSinkConfig {
    fn build_connector(&self) -> Result<MqttConnector, MqttError> {
        if self.client_id.is_empty() {
            return Err(ConfigurationError::EmptyClientId).context(ConfigurationSnafu);
        }
        let tls = MaybeTlsSettings::from_config(&self.tls, false).context(TlsSnafu)?;
        let mut options = MqttOptions::new(&self.client_id, &self.host, self.port);
        options.set_keep_alive(Duration::from_secs(self.keep_alive.into()));
        options.set_clean_session(self.clean_session);
        if let (Some(user), Some(password)) = (&self.user, &self.password) {
            options.set_credentials(user, password);
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
        MqttConnector::new(options, self.topic.to_string())
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
