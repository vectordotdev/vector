use crate::config::{DataType, GenerateConfig, SinkConfig, SinkContext};
use crate::http::HttpClient;
use crate::sinks::datadog::logs::healthcheck::healthcheck;
use crate::sinks::datadog::logs::service::DatadogLogsJsonService;
use crate::sinks::datadog::ApiKey;
use crate::sinks::datadog::Region;
use crate::sinks::util::encoding::EncodingConfigWithDefault;
use crate::sinks::util::{
    batch::{Batch, BatchError},
    buffer::GZIP_FAST,
    http::{HttpSink, PartitionHttpSink},
    BatchConfig, BatchSettings, Compression, JsonArrayBuffer, PartitionBuffer,
    PartitionInnerBuffer, TowerRequestConfig,
};
use crate::sinks::{Healthcheck, VectorSink};
use crate::tls::{MaybeTlsSettings, TlsConfig};
use flate2::write::GzEncoder;
use futures::{FutureExt, SinkExt};
use http::Request;
use indoc::indoc;
use serde::{Deserialize, Serialize};
use std::{io::Write, sync::Arc, time::Duration};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DatadogLogsConfig {
    pub(crate) endpoint: Option<String>,
    // Deprecated, replaced by the site option
    region: Option<Region>,
    site: Option<String>,
    // Deprecated name
    #[serde(alias = "api_key")]
    default_api_key: String,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub(crate) encoding: EncodingConfigWithDefault<Encoding>,
    tls: Option<TlsConfig>,

    #[serde(default)]
    compression: Option<Compression>,

    #[serde(default)]
    batch: BatchConfig,

    #[serde(default)]
    request: TowerRequestConfig,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Json,
}

impl GenerateConfig for DatadogLogsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            default_api_key = "${DATADOG_API_KEY_ENV_VAR}"
        "#})
        .unwrap()
    }
}

impl DatadogLogsConfig {
    fn get_uri(&self) -> String {
        self.endpoint
            .clone()
            .or_else(|| {
                self.site
                    .as_ref()
                    .map(|s| format!("https://http-intake.logs.{}/v1/input", s))
            })
            .unwrap_or_else(|| match self.region {
                Some(Region::Eu) => "https://http-intake.logs.datadoghq.eu/v1/input".to_string(),
                None | Some(Region::Us) => {
                    "https://http-intake.logs.datadoghq.com/v1/input".to_string()
                }
            })
    }

    fn batch_settings<T: Batch>(&self) -> Result<BatchSettings<T>, BatchError> {
        BatchSettings::default()
            .bytes(bytesize::mib(5_u32))
            .events(1_000)
            .timeout(15)
            .parse_config(self.batch)
    }

    /// Builds the required BatchedHttpSink.
    /// Since the DataDog sink can create one of two different sinks, this
    /// extracts most of the shared functionality required to create either sink.
    fn build_sink<T, B, O>(
        &self,
        cx: SinkContext,
        service: T,
        batch: B,
        timeout: Duration,
    ) -> crate::Result<(VectorSink, Healthcheck)>
    where
        O: 'static,
        B: Batch<Output = Vec<O>> + std::marker::Send + 'static,
        B::Output: std::marker::Send + Clone,
        B::Input: std::marker::Send,
        T: HttpSink<
                Input = PartitionInnerBuffer<B::Input, ApiKey>,
                Output = PartitionInnerBuffer<B::Output, ApiKey>,
            > + Clone,
    {
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());

        let tls_settings = MaybeTlsSettings::from_config(
            &Some(self.tls.clone().unwrap_or_else(TlsConfig::enabled)),
            false,
        )?;

        let client = HttpClient::new(tls_settings)?;
        let healthcheck = healthcheck(
            service.clone(),
            client.clone(),
            self.default_api_key.clone(),
        )
        .boxed();
        let sink = PartitionHttpSink::new(
            service,
            PartitionBuffer::new(batch),
            request_settings,
            timeout,
            client,
            cx.acker(),
        )
        .sink_map_err(|error| error!(message = "Fatal datadog_logs text sink error.", %error));
        let sink = VectorSink::Sink(Box::new(sink));

        Ok((sink, healthcheck))
    }

    /// Build the request, GZipping the contents if the config specifies.
    pub(crate) fn build_request(
        &self,
        uri: &str,
        api_key: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> crate::Result<http::Request<Vec<u8>>> {
        let request = Request::post(uri)
            .header("Content-Type", content_type)
            .header("DD-API-KEY", api_key);

        let compression = self.compression.unwrap_or(Compression::Gzip(None));

        let (request, body) = match compression {
            Compression::None => (request, body),
            Compression::Gzip(level) => {
                let level = level.unwrap_or(GZIP_FAST);
                let mut encoder =
                    GzEncoder::new(Vec::new(), flate2::Compression::new(level as u32));

                encoder.write_all(&body)?;
                (
                    request.header("Content-Encoding", "gzip"),
                    encoder.finish()?,
                )
            }
        };

        request
            .header("Content-Length", body.len())
            .body(body)
            .map_err(Into::into)
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_logs")]
impl SinkConfig for DatadogLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batch_settings = self.batch_settings()?;
        self.build_sink(
            cx,
            DatadogLogsJsonService {
                config: self.clone(),
                uri: self.get_uri(),
                default_api_key: Arc::from(self.default_api_key.clone()),
            },
            JsonArrayBuffer::new(batch_settings.size),
            batch_settings.timeout,
        )
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "datadog_logs"
    }
}

#[cfg(test)]
mod test {
    use crate::sinks::datadog::logs::DatadogLogsConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogLogsConfig>();
    }
}
