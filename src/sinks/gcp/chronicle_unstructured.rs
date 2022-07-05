use bytes::Bytes;
use futures_util::{future::BoxFuture, task::Poll};
use goauth::scopes::Scope;
use http::{header::HeaderValue, Request, Uri};
use hyper::Body;
use indoc::indoc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use snafu::Snafu;
use std::io;
use tower::{Service, ServiceBuilder};
use vector_buffers::Ackable;
use vector_core::{
    config::{AcknowledgementsConfig, Input},
    event::{Event, EventFinalizers, Finalizable},
    sink::VectorSink,
};

use crate::{
    config::{log_schema, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    gcp::{GcpAuthConfig, GcpCredentials},
    http::{HttpClient, HttpError},
    sinks::{
        gcs_common::{
            config::{healthcheck_response, GcsRetryLogic},
            service::{GcsMetadata, GcsResponse},
            sink::GcsSink,
        },
        util::{
            encoding::{
                Encoder, EncodingConfig, EncodingConfigWithFramingAdapter, StandardEncodings,
                StandardEncodingsWithFramingMigrator,
            },
            partitioner::KeyPartitioner,
            BatchConfig, BulkSizeBasedDefaultBatchSettings, Compression, RequestBuilder,
            TowerRequestConfig,
        },
        Healthcheck,
    },
    template::{Template, TemplateParseError},
    tls::{TlsConfig, TlsSettings},
};

const NAME: &str = "gcp_chronicle_unstructured";

// https://cloud.google.com/chronicle/docs/reference/ingestion-api#ingestion_api_reference
// We can send UDM (unified data model - https://cloud.google.com/chronicle/docs/reference/udm-field-list)
// events or unstructured log entries.
// const CHRONICLE_URL: &str = "https://malachiteingestion-pa.googleapis.com";

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GcsHealthcheckError {
    #[snafu(display("log_type template parse error: {}", source))]
    LogTypeTemplate { source: TemplateParseError },

    #[snafu(display("Endpoint not found"))]
    NotFound,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Eu,
    Us,
    Asia,
}

impl Region {
    /// Each region has a it's own endpoint.
    fn endpoint(self) -> &'static str {
        match self {
            Region::Eu => "https://europe-malachiteingestion-pa.googleapis.com",
            Region::Us => "https://malachiteingestion-pa.googleapis.com",
            Region::Asia => "https://asia-southeast1-malachiteingestion-pa.googleapis.com",
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct GcsChronicleUnstructuredConfig {
    pub endpoint: Option<String>,
    pub region: Option<Region>,
    pub customer_id: String,
    #[serde(default = "default_skip_authentication")]
    pub skip_authentication: bool,
    #[serde(flatten)]
    pub auth: GcpAuthConfig,
    #[serde(default)]
    pub batch: BatchConfig<BulkSizeBasedDefaultBatchSettings>,
    #[serde(flatten)]
    pub encoding: EncodingConfigWithFramingAdapter<
        EncodingConfig<StandardEncodings>,
        StandardEncodingsWithFramingMigrator,
    >,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(default)]
    compression: Compression,
    pub tls: Option<TlsConfig>,
    pub log_type: Template,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

const fn default_skip_authentication() -> bool {
    false
}

inventory::submit! {
    SinkDescription::new::<GcsChronicleUnstructuredConfig>(NAME)
}

impl GenerateConfig for GcsChronicleUnstructuredConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            credentials_path = "/path/to/credentials.json"
            encoding.codec = "ndjson"
        "#})
        .unwrap()
    }
}

pub fn build_healthcheck(
    client: HttpClient,
    base_url: &str,
    creds: Option<GcpCredentials>,
) -> crate::Result<Healthcheck> {
    let uri = base_url.parse::<Uri>()?;

    let healthcheck = async move {
        let mut request = http::Request::get(&uri).body(Body::empty())?;

        if let Some(creds) = creds.as_ref() {
            creds.apply(&mut request);
        }

        let response = client.send(request).await?;
        healthcheck_response(response, creds, GcsHealthcheckError::NotFound.into())
    };

    Ok(Box::pin(healthcheck))
}

#[derive(Debug, Snafu)]
pub enum ChronicleError {
    #[snafu(display("Region or endpoint not defined"))]
    RegionOrEndpoint,
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_chronicle_unstructured")]
impl SinkConfig for GcsChronicleUnstructuredConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let creds = if self.skip_authentication {
            None
        } else {
            self.auth
                .make_credentials(Scope::MalachiteIngestion)
                .await?
        };

        let tls = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls, cx.proxy())?;

        let endpoint = self.endpoint("v2/unstructuredlogentries:batchCreate")?;

        // For the healthcheck we see if we can fetch the list of available log types.
        let healthcheck_endpoint = self.endpoint("v2/logtypes")?;

        let healthcheck = build_healthcheck(client.clone(), &healthcheck_endpoint, creds.clone())?;
        let sink = self.build_sink(client, endpoint, creds, cx)?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        NAME
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
    }
}

impl GcsChronicleUnstructuredConfig {
    fn build_sink(
        &self,
        client: HttpClient,
        base_url: String,
        creds: Option<GcpCredentials>,
        cx: SinkContext,
    ) -> crate::Result<VectorSink> {
        use crate::sinks::util::service::ServiceBuilderExt;

        let request = self.request.unwrap_with(&TowerRequestConfig {
            rate_limit_num: Some(1000),
            ..Default::default()
        });

        let batch_settings = self.batch.into_batcher_settings()?;

        let partitioner = self.key_partitioner()?;

        let svc = ServiceBuilder::new()
            .settings(request, GcsRetryLogic)
            .service(ChronicleService::new(client, base_url, creds));

        let request_settings = RequestSettings::new(self)?;

        let sink = GcsSink::new(cx, svc, request_settings, partitioner, batch_settings);

        Ok(VectorSink::from_event_streamsink(sink))
    }

    fn key_partitioner(&self) -> crate::Result<KeyPartitioner> {
        Ok(KeyPartitioner::new(self.log_type.clone()))
    }

    fn endpoint(&self, path: &str) -> Result<String, ChronicleError> {
        Ok(format!(
            "{}/{}",
            match (&self.endpoint, self.region) {
                (Some(endpoint), None) => endpoint.trim_end_matches("/"),
                (None, Some(region)) => region.endpoint(),
                _ => return Err(ChronicleError::RegionOrEndpoint),
            },
            path
        ))
    }
}

#[derive(Clone, Debug)]
pub struct ChronicleRequest {
    pub body: Bytes,
    pub metadata: GcsMetadata,
}

impl Ackable for ChronicleRequest {
    fn ack_size(&self) -> usize {
        self.metadata.count
    }
}

impl Finalizable for ChronicleRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

#[derive(Clone, Debug)]
struct ChronicleEncoder {
    customer_id: String,
}

impl Encoder<(String, Vec<Event>)> for ChronicleEncoder {
    fn encode_input(
        &self,
        input: (String, Vec<Event>),
        writer: &mut dyn io::Write,
    ) -> io::Result<usize> {
        let (partition_key, events) = input;
        let events = events
            .into_iter()
            .map(|event| {
                let log = event.into_log();
                let message = log.get(log_schema().message_key()).unwrap().to_string();

                let mut value = json!({
                    "log_text": message,
                });

                if let Some(ts) = log.get(log_schema().timestamp_key()) {
                    if let Some(ts) = ts.as_timestamp() {
                        value.as_object_mut().unwrap().insert(
                            "ts_rfc3339".to_string(),
                            ts.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
                                .into(),
                        );
                    }
                }

                value
            })
            .collect::<Vec<_>>();

        let json = json!({
            "customer_id": self.customer_id,
            "log_type": partition_key,
            "entries": events,
        });

        let body = crate::serde::json::to_bytes(&json)?.freeze();
        writer.write(&body)
    }
}

// Settings required to produce a request that do not change per
// request. All possible values are pre-computed for direct use in
// producing a request.
#[derive(Clone, Debug)]
struct RequestSettings {
    encoder: ChronicleEncoder, //(Transformer, Encoder<Framer>),

    // TODO Does chronicle handle compression?
    compression: Compression,
}

impl RequestBuilder<(String, Vec<Event>)> for RequestSettings {
    type Metadata = GcsMetadata;
    type Events = (String, Vec<Event>);
    type Encoder = ChronicleEncoder;
    type Payload = Bytes;
    type Request = ChronicleRequest;
    type Error = io::Error; // TODO: this is ugly.

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, input: (String, Vec<Event>)) -> (Self::Metadata, Self::Events) {
        use vector_core::ByteSizeOf;

        let (partition_key, mut events) = input;
        let finalizers = events.take_finalizers();

        let metadata = GcsMetadata {
            key: partition_key.clone(),
            count: events.len(),
            byte_size: events.size_of(),
            finalizers,
        };
        (metadata, (partition_key, events))
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        trace!(message = "Sending events.", bytes = ?payload.len(), events_len = ?metadata.count, key = ?metadata.key);

        ChronicleRequest {
            body: payload,
            metadata,
        }
    }
}

impl RequestSettings {
    fn new(config: &GcsChronicleUnstructuredConfig) -> crate::Result<Self> {
        Ok(Self {
            compression: config.compression,
            encoder: ChronicleEncoder {
                customer_id: config.customer_id.clone(),
            },
        })
    }
}

#[derive(Debug, Clone)]
pub struct ChronicleService {
    client: HttpClient,
    base_url: String,
    creds: Option<GcpCredentials>,
}

impl ChronicleService {
    pub const fn new(client: HttpClient, base_url: String, creds: Option<GcpCredentials>) -> Self {
        Self {
            client,
            base_url,
            creds,
        }
    }
}

impl Service<ChronicleRequest> for ChronicleService {
    type Response = GcsResponse;
    type Error = HttpError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: ChronicleRequest) -> Self::Future {
        let mut builder = Request::post(&self.base_url);
        let headers = builder.headers_mut().unwrap();
        headers.insert(
            "content-type",
            HeaderValue::from_str("application/json").unwrap(),
        );
        headers.insert(
            "content-length",
            HeaderValue::from_str(&request.body.len().to_string()).unwrap(),
        );

        let mut http_request = builder.body(Body::from(request.body)).unwrap();
        if let Some(creds) = &self.creds {
            creds.apply(&mut http_request);
        }

        let mut client = self.client.clone();
        Box::pin(async move {
            let result = client.call(http_request).await;
            result.map(|inner| GcsResponse {
                inner,
                count: request.metadata.count,
                events_byte_size: request.metadata.byte_size,
            })
        })
    }
}
