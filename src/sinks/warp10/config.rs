use crate::{
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        util::{
            http::{BatchedHttpSink, RequestConfig},
            BatchConfig, BatchSettings, Buffer, Compression, RealtimeSizeBasedDefaultBatchSettings,
            TowerRequestConfig, TowerRequestSettings,
        },
        warp10::sink::Warp10Sink,
        Healthcheck, VectorSink,
    },
};
use futures_util::{future, SinkExt};
use vector_config::configurable_component;
use vector_core::tls::TlsSettings;

/// Configuration for the `warp10` sink.
#[configurable_component(sink("warp10"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Warp10SinkConfig {
    /// The Warp10 update URI to connect to.
    ///
    /// This should include the protocol and host, port and path.
    #[configurable(derived)]
    pub uri: String,

    /// The Warp10 write token used to push metrics
    #[configurable(derived)]
    pub token: String,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for Warp10SinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            uri: "http://127.0.0.1:8080/api/v0/update".into(),
            token: "WRITE_TOKEN".into(),
            acknowledgements: AcknowledgementsConfig::DEFAULT,
            batch: BatchConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for Warp10SinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let tls: TlsSettings = TlsSettings::default();
        let client: HttpClient = HttpClient::new(tls, cx.proxy())?;
        let healthcheck: Healthcheck = Box::pin(future::ok(()));
        let request: RequestConfig = RequestConfig::default();

        let batch: BatchSettings<Buffer> = self.batch.into_batch_settings()?;

        let warp10_sink = Warp10Sink {
            uri: self.uri.clone(),
            token: self.token.clone(),
        };

        let request: TowerRequestSettings =
            request.tower.unwrap_with(&TowerRequestConfig::default());

        let sink = BatchedHttpSink::new(
            warp10_sink,
            Buffer::new(batch.size, Compression::None),
            request,
            batch.timeout,
            client,
        )
        .sink_map_err(|error| error!(message = "Fatal HTTP sink error.", %error));

        let sink: VectorSink = VectorSink::from_event_sink(sink);
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
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
        crate::test_util::test_generate_config::<Warp10SinkConfig>();
    }
}
