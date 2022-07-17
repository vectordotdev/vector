use codecs::JsonSerializerConfig;
use rumqttc::{MqttOptions, Transport};
use serde::{Deserialize, Serialize};
use snafu::ResultExt;

use crate::{
    codecs::EncodingConfig,
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{
        mqtt::sink::{TlsSnafu, MqttConnector, MqttError, MqttSink},
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MqttSinkConfig {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub user: String,
    pub password: String,
    #[serde(default = "default_client_id")]
    pub client_id: String,
    #[serde(default = "default_keep_alive")]
    pub keep_alive: u16,
    #[serde(default = "default_clean_session")]
    pub clean_session: bool,
    pub tls: Option<TlsEnableableConfig>,
    pub topic: String,
    pub encoding: EncodingConfig,
}

const fn default_port() -> u16 {
    1883
}

fn default_client_id() -> String {
    "vector".into()
}

const fn default_keep_alive() -> u16 {
    60
}

const fn default_clean_session() -> bool {
    false
}

impl GenerateConfig for MqttSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            host: "localhost".into(),
            port: default_port(),
            user: "admin".into(),
            password: "secret".into(),
            client_id: default_client_id(),
            keep_alive: default_keep_alive(),
            clean_session: default_clean_session(),
            tls: None,
            topic: "vector".into(),
            encoding: JsonSerializerConfig::new().into(),
        })
        .unwrap()
    }
}

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

    fn sink_type(&self) -> &'static str {
        "mqtt"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        None
    }
}

impl MqttSinkConfig {
    fn build_connector(&self) -> Result<MqttConnector, MqttError> {
        let tls = MaybeTlsSettings::from_config(&self.tls, false)
            .context(TlsSnafu)?;
        let mut options = MqttOptions::new(&self.client_id, &self.host, self.port);
        options.set_keep_alive(self.keep_alive);
        options.set_clean_session(self.clean_session);
        options.set_credentials(&self.user, &self.password);
        if let Some(tls) = tls.tls() {
            let ca = tls.authorities_pem().flatten().collect();
            let client_auth = None;
            let alpn = Some(vec!["mqtt".into()]);
            options.set_transport(Transport::tls(ca, client_auth, alpn));
        }
        MqttConnector::new(options, self.topic.clone())
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
