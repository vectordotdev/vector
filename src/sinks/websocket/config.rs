use serde::{Deserialize, Serialize};
use snafu::ResultExt;

use crate::{
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{
        util::encoding::{
            EncodingConfig, EncodingConfigAdapter, StandardEncodings, StandardEncodingsMigrator,
        },
        websocket::sink::{ConnectSnafu, WebSocketConnector, WebSocketError, WebSocketSink},
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WebSocketSinkConfig {
    pub uri: String,
    pub tls: Option<TlsEnableableConfig>,
    #[serde(flatten)]
    pub encoding:
        EncodingConfigAdapter<EncodingConfig<StandardEncodings>, StandardEncodingsMigrator>,
    pub ping_interval: Option<u64>,
    pub ping_timeout: Option<u64>,
}

impl GenerateConfig for WebSocketSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            uri: "ws://127.0.0.1:9000/endpoint".into(),
            tls: None,
            encoding: EncodingConfig::from(StandardEncodings::Json).into(),
            ping_interval: None,
            ping_timeout: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "websocket")]
impl SinkConfig for WebSocketSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let connector = self.build_connector()?;
        let ws_sink = WebSocketSink::new(self, connector.clone(), cx.acker());

        Ok((
            VectorSink::from_event_streamsink(ws_sink),
            Box::pin(async move { connector.healthcheck().await }),
        ))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "websocket"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        None
    }
}

impl WebSocketSinkConfig {
    fn build_connector(&self) -> Result<WebSocketConnector, WebSocketError> {
        let tls = MaybeTlsSettings::from_config(&self.tls, false).context(ConnectSnafu)?;
        WebSocketConnector::new(self.uri.clone(), tls)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<WebSocketSinkConfig>();
    }
}
