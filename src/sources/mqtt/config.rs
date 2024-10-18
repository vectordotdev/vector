use std::time::Duration;

use rand::Rng;
use rumqttc::{MqttOptions, TlsConfiguration, Transport};
use snafu::{ResultExt, Snafu};
use vector_lib::{
    codecs::decoding::{DeserializerConfig, FramingConfig},
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::owned_value_path,
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};
use vrl::value::Kind;

use crate::{
    codecs::DecodingConfig,
    config::{SourceConfig, SourceContext, SourceOutput},
    serde::{default_decoding, default_framing_message_based},
};

use super::source::{ConfigurationSnafu, MqttConnector, MqttError, MqttSource, TlsSnafu};

#[derive(Clone, Debug, Eq, PartialEq, Snafu)]
pub enum ConfigurationError {
    #[snafu(display(
        "Client ID must be 1-23 characters long and must consist of only alphanumeric characters."
    ))]
    InvalidClientId,

    #[snafu(display("Username and password must be either both or neither provided."))]
    BadCredentials,
}

/// Configuration for the `mqtt` source.
#[configurable_component(source("mqtt", "Collect logs from MQTT."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct MqttSourceConfig {
    /// MQTT server address (The broker’s domain name or IP address).
    #[configurable(metadata(docs::examples = "mqtt.example.com", docs::examples = "127.0.0.1"))]
    pub host: String,

    /// TCP port of the MQTT server to connect to.
    #[configurable(derived)]
    #[serde(default = "default_port")]
    #[derivative(Default(value = "default_port()"))]
    pub port: u16,

    /// MQTT username.
    #[configurable(derived)]
    #[serde(default)]
    pub user: Option<String>,

    /// MQTT password.
    #[configurable(derived)]
    #[serde(default)]
    pub password: Option<String>,

    /// MQTT client ID. If there are multiple
    #[configurable(derived)]
    #[serde(default)]
    pub client_id: Option<String>,

    /// Connection keep-alive interval.
    #[configurable(derived)]
    #[serde(default = "default_keep_alive")]
    #[derivative(Default(value = "default_keep_alive()"))]
    pub keep_alive: u16,

    /// MQTT topic from which messages are to be read.
    #[configurable(derived)]
    #[serde(default = "default_topic")]
    #[derivative(Default(value = "default_topic()"))]
    pub topic: String,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,

    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

const fn default_port() -> u16 {
    1883
}

const fn default_keep_alive() -> u16 {
    60
}

fn default_topic() -> String {
    "vector".to_owned()
}

#[async_trait::async_trait]
#[typetag::serde(name = "mqtt")]
impl SourceConfig for MqttSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let connector = self.build_connector()?;

        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let sink = MqttSource::new(connector.clone(), decoder, log_namespace)?;
        Ok(Box::pin(sink.run(cx.out, cx.shutdown)))
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

        vec![SourceOutput::new_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

impl MqttSourceConfig {
    fn build_connector(&self) -> Result<MqttConnector, MqttError> {
        let client_id = self.client_id.clone().unwrap_or_else(|| {
            let hash = rand::thread_rng()
                .sample_iter(&rand_distr::Alphanumeric)
                .take(6)
                .map(char::from)
                .collect::<String>();
            format!("vectorSource{hash}")
        });

        if client_id.is_empty() {
            return Err(ConfigurationError::InvalidClientId).context(ConfigurationSnafu);
        }

        let tls = MaybeTlsSettings::from_config(&self.tls, false).context(TlsSnafu)?;
        let mut options = MqttOptions::new(client_id, &self.host, self.port);
        options.set_keep_alive(Duration::from_secs(self.keep_alive.into()));
        options.set_clean_session(false);
        match (&self.user, &self.password) {
            (Some(user), Some(password)) => {
                options.set_credentials(user, password);
            }
            (None, None) => {
                // Credentials were not provided
            }
            _ => {
                // We need either both username and password, or neither. MQTT also allows for providing only password, but rumqttc does not allow that so we cannot either.
                return Err(ConfigurationError::BadCredentials).context(ConfigurationSnafu);
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

        MqttConnector::new(options, self.topic.clone())
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
