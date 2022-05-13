use futures::{FutureExt, SinkExt};
use http::{Request, Uri};
use hyper::Body;
use serde::{Deserialize, Serialize};
use serde_json::value::{RawValue, Value};
use snafu::Snafu;
use vector_core::partition::Partitioner;

use super::{encoder::Encoding, sink::ChronicleSink};
use crate::{
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext, SinkDescription},
    gcp::{GcpAuthConfig, GcpCredentials},
    http::HttpClient,
    sinks::{
        gcs_common::config::healthcheck_response,
        util::{
            encoding::EncodingConfigWithDefault, http::BatchedHttpSink, http::PartitionHttpSink,
            Batch, BatchConfig, JsonArrayBuffer, PartitionBuffer, PushResult, SinkBatchSettings,
            TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    tls::{TlsConfig, TlsSettings},
};

// 10MB maximum message size: https://cloud.google.com/pubsub/quotas#resource_limits
const MAX_BATCH_PAYLOAD_SIZE: usize = 10_000_000;

pub type BoxedRawValue = Box<RawValue>;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ChronicleSinkConfig {
    pub endpoint: Option<String>,
    #[serde(default = "default_skip_authentication")]
    pub skip_authentication: bool,
    #[serde(flatten)]
    pub auth: GcpAuthConfig,
    #[serde(default)]
    pub batch: BatchConfig<ChronicleDefaultBatchSettings>,
    // TODO Encoding is probably not needed?
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsConfig>,
}

impl_generate_config_from_default!(ChronicleSinkConfig);

inventory::submit! {
    SinkDescription::new::<ChronicleSinkConfig>("chronicle")
}

const fn default_skip_authentication() -> bool {
    false
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ChronicleDefaultBatchSettings;

impl SinkBatchSettings for ChronicleDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1000);
    const MAX_BYTES: Option<usize> = Some(1_000_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

#[async_trait::async_trait]
#[typetag::serde(name = "chronicle")]
impl SinkConfig for ChronicleSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = ChronicleSink::from_config(self).await?;
        let batch_settings = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_BATCH_PAYLOAD_SIZE)?
            .into_batch_settings()?;

        let request_settings = self.request.unwrap_with(&Default::default());
        let tls_settings = TlsSettings::from_options(&self.tls)?;

        let client = HttpClient::new(tls_settings, cx.proxy())?;
        let healthcheck = healthcheck(client.clone(), sink.uri("")?, sink.creds.clone()).boxed();


        /*
        let sink = BatchedHttpSink::new(
            sink,
            JsonArrayBuffer::new(batch_settings.size),
            request_settings,
            batch_settings.timeout,
            client,
            cx.acker(),
        )
        .sink_map_err(|error| error!(message = "Fatal chronicle sink error.", %error));
        */
        let sink = PartitionHttpSink::new(
            sink,
            PartitionBuffer::new(JsonArrayBuffer::new(batch_settings.size)),
            request_settings,
            batch_settings.timeout,
            client,
            cx.acker(),
        )
        .sink_map_err(|error| error!(message = "Fatal chronicle sink error.", %error));

        Ok((VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "chronicle"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        // TODO This probably could be.
        None
    }
}

// TODO I dont think this is the right healthcheck error.
#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Configured topic not found"))]
    TopicNotFound,
}

async fn healthcheck(
    client: HttpClient,
    uri: Uri,
    creds: Option<GcpCredentials>,
) -> crate::Result<()> {
    let mut request = Request::get(uri).body(Body::empty()).unwrap();
    if let Some(creds) = creds.as_ref() {
        creds.apply(&mut request);
    }

    let response = client.send(request).await?;
    healthcheck_response(response, creds, HealthcheckError::TopicNotFound.into())
}
