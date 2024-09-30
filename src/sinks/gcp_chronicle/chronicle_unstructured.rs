//! This sink sends data to Google Chronicles unstructured log entries endpoint.
//! See <https://cloud.google.com/chronicle/docs/reference/ingestion-api#unstructuredlogentries>
//! for more information.
use bytes::{Bytes, BytesMut};
use std::{cell::RefCell, collections::BTreeSet};

use futures_util::{future::BoxFuture, task::Poll};
use goauth::scopes::Scope;
use http::{header::HeaderValue, Request, StatusCode, Uri};
use hyper::Body;
use indexmap::IndexMap;
use indoc::indoc;
use serde::Serialize;
use serde_json::json;
use snafu::Snafu;
use std::collections::HashMap;
use std::io;
use tokio_util::codec::Encoder as _;
use tower::{Service, ServiceBuilder};
use vector_lib::configurable::attributes::CustomAttribute;
use vector_lib::configurable::{
    schema::{
        apply_base_metadata, generate_const_string_schema, generate_enum_schema,
        generate_one_of_schema, generate_struct_schema, get_or_generate_schema, SchemaGenerator,
        SchemaObject,
    },
    Configurable, GenerateError, Metadata, ToValue,
};
use vector_lib::configurable::{configurable_component, Configurable, GenerateError, Metadata};
use vector_lib::request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata};
use vector_lib::{
    config::{telemetry, AcknowledgementsConfig, Input},
    event::{Event, EventFinalizers, Finalizable},
    sink::VectorSink,
    EstimatedJsonEncodedSizeOf,
};
use vrl::value::Kind;

use crate::sinks::util::buffer::compression::CompressionLevel;
use crate::sinks::util::service::TowerRequestConfigDefaults;
use crate::{
    codecs::{self, EncodingConfig},
    config::{GenerateConfig, SinkConfig, SinkContext},
    gcp::{GcpAuthConfig, GcpAuthenticator},
    http::HttpClient,
    schema,
    sinks::{
        gcp_chronicle::{
            partitioner::{ChroniclePartitionKey, ChroniclePartitioner},
            sink::ChronicleSink,
        },
        gcs_common::{
            config::{healthcheck_response, GcsRetryLogic},
            service::GcsResponse,
        },
        util::{
            encoding::{as_tracked_write, Encoder},
            metadata::RequestMetadataBuilder,
            request_builder::EncodeResult,
            BatchConfig, Compression, RequestBuilder, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck,
    },
    template::{Template, TemplateParseError},
    tls::{TlsConfig, TlsSettings},
};

/// Compression configuration.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[derivative(Default)]
#[serde(rename_all = "snake_case")]
#[configurable(metadata(
    docs::enum_tag_description = "The compression algorithm to use for sending."
))]
pub enum ChronicleCompression {
    /// No compression.
    #[derivative(Default)]
    None,

    /// [Gzip][gzip] compression.
    ///
    /// [gzip]: https://www.gzip.org/
    Gzip(CompressionLevel),
}

impl From<ChronicleCompression> for Compression {
    fn from(compression: ChronicleCompression) -> Self {
        match compression {
            ChronicleCompression::None => Compression::None,
            ChronicleCompression::Gzip(compression_level) => Compression::Gzip(compression_level),
        }
    }
}

// Schema generation largely copied from `src/sinks/util/buffer/compression`
impl Configurable for ChronicleCompression {
    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_title("Compression configuration.");
        metadata.set_description("All compression algorithms use the default compression level unless otherwise specified.");
        metadata.add_custom_attribute(CustomAttribute::kv("docs::enum_tagging", "external"));
        metadata.add_custom_attribute(CustomAttribute::flag("docs::advanced"));
        metadata
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        const ALGORITHM_NAME: &str = "algorithm";
        const LEVEL_NAME: &str = "level";
        const LOGICAL_NAME: &str = "logical_name";
        const ENUM_TAGGING_MODE: &str = "docs::enum_tagging";

        let generate_string_schema = |logical_name: &str,
                                      title: Option<&'static str>,
                                      description: &'static str|
         -> SchemaObject {
            let mut const_schema = generate_const_string_schema(logical_name.to_lowercase());
            let mut const_metadata = Metadata::with_description(description);
            if let Some(title) = title {
                const_metadata.set_title(title);
            }
            const_metadata.add_custom_attribute(CustomAttribute::kv(LOGICAL_NAME, logical_name));
            apply_base_metadata(&mut const_schema, const_metadata);
            const_schema
        };

        // First, we'll create the string-only subschemas for each algorithm, and wrap those up
        // within a one-of schema.
        let mut string_metadata = Metadata::with_description("Compression algorithm.");
        string_metadata.add_custom_attribute(CustomAttribute::kv(ENUM_TAGGING_MODE, "external"));

        let none_string_subschema = generate_string_schema("None", None, "No compression.");
        let gzip_string_subschema = generate_string_schema(
            "Gzip",
            Some("[Gzip][gzip] compression."),
            "[gzip]: https://www.gzip.org/",
        );

        let mut all_string_oneof_subschema = generate_one_of_schema(&[none_string_subschema, gzip_string_subschema]);
        apply_base_metadata(&mut all_string_oneof_subschema, string_metadata);

        let compression_level_schema =
            get_or_generate_schema(&CompressionLevel::as_configurable_ref(), gen, None)?;

        let mut required = BTreeSet::new();
        required.insert(ALGORITHM_NAME.to_string());

        let mut properties = IndexMap::new();
        properties.insert(
            ALGORITHM_NAME.to_string(),
            all_string_oneof_subschema.clone(),
        );
        properties.insert(LEVEL_NAME.to_string(), compression_level_schema);

        let mut full_subschema = generate_struct_schema(properties, required, None);
        let mut full_metadata =
            Metadata::with_description("Compression algorithm and compression level.");
        full_metadata.add_custom_attribute(CustomAttribute::flag("docs::hidden"));
        apply_base_metadata(&mut full_subschema, full_metadata);

        Ok(generate_one_of_schema(&[
            all_string_oneof_subschema,
            full_subschema,
        ]))
    }
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GcsHealthcheckError {
    #[snafu(display("log_type template parse error: {}", source))]
    LogTypeTemplate { source: TemplateParseError },

    #[snafu(display("Endpoint not found"))]
    NotFound,
}

/// Google Chronicle regions.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// EU region.
    Eu,

    /// US region.
    Us,

    /// APAC region.
    Asia,
}

impl Region {
    /// Each region has a its own endpoint.
    const fn endpoint(self) -> &'static str {
        match self {
            Region::Eu => "https://europe-malachiteingestion-pa.googleapis.com",
            Region::Us => "https://malachiteingestion-pa.googleapis.com",
            Region::Asia => "https://asia-southeast1-malachiteingestion-pa.googleapis.com",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ChronicleUnstructuredDefaultBatchSettings;

// Chronicle Ingestion API has a 1MB limit[1] for unstructured log entries. We're also using a
// conservatively low batch timeout to ensure events make it to Chronicle in a timely fashion, but
// high enough that it allows for reasonable batching.
//
// [1]: https://cloud.google.com/chronicle/docs/reference/ingestion-api#unstructuredlogentries
impl SinkBatchSettings for ChronicleUnstructuredDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(1_000_000);
    const TIMEOUT_SECS: f64 = 15.0;
}

#[derive(Clone, Copy, Debug)]
pub struct ChronicleUnstructuredTowerRequestConfigDefaults;

impl TowerRequestConfigDefaults for ChronicleUnstructuredTowerRequestConfigDefaults {
    const RATE_LIMIT_NUM: u64 = 1_000;
}
/// Configuration for the `gcp_chronicle_unstructured` sink.
#[configurable_component(sink(
    "gcp_chronicle_unstructured",
    "Store unstructured log events in Google Chronicle."
))]
#[derive(Clone, Debug)]
pub struct ChronicleUnstructuredConfig {
    /// The endpoint to send data to.
    #[configurable(metadata(
        docs::examples = "127.0.0.1:8080",
        docs::examples = "example.com:12345"
    ))]
    pub endpoint: Option<String>,

    /// The GCP region to use.
    #[configurable(derived)]
    pub region: Option<Region>,

    /// The Unique identifier (UUID) corresponding to the Chronicle instance.
    #[configurable(validation(format = "uuid"))]
    #[configurable(metadata(docs::examples = "c8c65bfa-5f2c-42d4-9189-64bb7b939f2c"))]
    pub customer_id: String,

    /// User-configured environment namespace to identify the data domain the logs originated from.
    #[configurable(metadata(docs::templateable))]
    #[configurable(metadata(
        docs::examples = "production",
        docs::examples = "production-{{ namespace }}",
    ))]
    #[configurable(metadata(docs::advanced))]
    pub namespace: Option<Template>,

    /// A set of labels that are attached to each batch of events.
    #[configurable(metadata(docs::examples = "chronicle_labels_examples()"))]
    #[configurable(metadata(docs::additional_props_description = "A Chronicle label."))]
    pub labels: Option<HashMap<String, String>>,

    #[serde(flatten)]
    pub auth: GcpAuthConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<ChronicleUnstructuredDefaultBatchSettings>,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: ChronicleCompression,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig<ChronicleUnstructuredTowerRequestConfigDefaults>,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    /// The type of log entries in a request.
    ///
    /// This must be one of the [supported log types][unstructured_log_types_doc], otherwise
    /// Chronicle rejects the entry with an error.
    ///
    /// [unstructured_log_types_doc]: https://cloud.google.com/chronicle/docs/ingestion/parser-list/supported-default-parsers
    #[configurable(metadata(docs::examples = "WINDOWS_DNS", docs::examples = "{{ log_type }}"))]
    pub log_type: Template,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn chronicle_labels_examples() -> HashMap<String, String> {
    let mut examples = HashMap::new();
    examples.insert("source".to_string(), "vector".to_string());
    examples.insert("tenant".to_string(), "marketing".to_string());
    examples
}

impl GenerateConfig for ChronicleUnstructuredConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            credentials_path = "/path/to/credentials.json"
            customer_id = "customer_id"
            namespace = "namespace"
            compression = gzip
            log_type = "log_type"
            encoding.codec = "text"
        "#})
        .unwrap()
    }
}

pub fn build_healthcheck(
    client: HttpClient,
    base_url: &str,
    auth: GcpAuthenticator,
) -> crate::Result<Healthcheck> {
    let uri = base_url.parse::<Uri>()?;

    let healthcheck = async move {
        let mut request = http::Request::get(&uri).body(Body::empty())?;
        auth.apply(&mut request);

        let response = client.send(request).await?;
        healthcheck_response(response, GcsHealthcheckError::NotFound.into())
    };

    Ok(Box::pin(healthcheck))
}

#[derive(Debug, Snafu)]
pub enum ChronicleError {
    #[snafu(display("Region or endpoint not defined"))]
    RegionOrEndpoint,
    #[snafu(display("You can only specify one of region or endpoint"))]
    BothRegionAndEndpoint,
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_chronicle_unstructured")]
impl SinkConfig for ChronicleUnstructuredConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let creds = self.auth.build(Scope::MalachiteIngestion).await?;

        let tls = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls, cx.proxy())?;

        let endpoint = self.create_endpoint("v2/unstructuredlogentries:batchCreate")?;

        // For the healthcheck we see if we can fetch the list of available log types.
        let healthcheck_endpoint = self.create_endpoint("v2/logtypes")?;

        let healthcheck = build_healthcheck(client.clone(), &healthcheck_endpoint, creds.clone())?;
        creds.spawn_regenerate_token();
        let sink = self.build_sink(client, endpoint, creds)?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        let requirement =
            schema::Requirement::empty().required_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl ChronicleUnstructuredConfig {
    fn build_sink(
        &self,
        client: HttpClient,
        base_url: String,
        creds: GcpAuthenticator,
    ) -> crate::Result<VectorSink> {
        use crate::sinks::util::service::ServiceBuilderExt;

        let request = self.request.into_settings();

        let batch_settings = self.batch.into_batcher_settings()?;

        let partitioner = self.partitioner()?;

        let svc = ServiceBuilder::new()
            .settings(request, GcsRetryLogic)
            .service(ChronicleService::new(client, base_url, creds));

        let request_settings = ChronicleRequestBuilder::new(self)?;

        let sink = ChronicleSink::new(svc, request_settings, partitioner, batch_settings, "http");

        Ok(VectorSink::from_event_streamsink(sink))
    }

    fn partitioner(&self) -> crate::Result<ChroniclePartitioner> {
        Ok(ChroniclePartitioner::new(
            self.log_type.clone(),
            self.namespace.clone(),
        ))
    }

    fn create_endpoint(&self, path: &str) -> Result<String, ChronicleError> {
        Ok(format!(
            "{}/{}",
            match (&self.endpoint, self.region) {
                (Some(endpoint), None) => endpoint.trim_end_matches('/'),
                (None, Some(region)) => region.endpoint(),
                (Some(_), Some(_)) => return Err(ChronicleError::BothRegionAndEndpoint),
                (None, None) => return Err(ChronicleError::RegionOrEndpoint),
            },
            path
        ))
    }
}

#[derive(Clone, Debug)]
pub struct ChronicleRequest {
    pub body: Bytes,
    pub finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

impl Finalizable for ChronicleRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for ChronicleRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

#[derive(Clone, Debug, Serialize)]
struct ChronicleRequestBody {
    customer_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    labels: Option<Vec<Label>>,
    log_type: String,
    entries: Vec<serde_json::Value>,
}

#[derive(Clone, Debug)]
struct ChronicleEncoder {
    customer_id: String,
    labels: Option<Vec<Label>>,
    encoder: codecs::Encoder<()>,
    transformer: codecs::Transformer,
}

impl Encoder<(ChroniclePartitionKey, Vec<Event>)> for ChronicleEncoder {
    fn encode_input(
        &self,
        input: (ChroniclePartitionKey, Vec<Event>),
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let (key, events) = input;
        let mut encoder = self.encoder.clone();
        let mut byte_size = telemetry().create_request_count_byte_size();
        let events = events
            .into_iter()
            .filter_map(|mut event| {
                let timestamp = event
                    .as_log()
                    .get_timestamp()
                    .and_then(|ts| ts.as_timestamp())
                    .cloned();
                let mut bytes = BytesMut::new();
                self.transformer.transform(&mut event);

                byte_size.add_event(&event, event.estimated_json_encoded_size_of());

                encoder.encode(event, &mut bytes).ok()?;

                let mut value = json!({
                    "log_text": String::from_utf8_lossy(&bytes),
                });

                if let Some(ts) = timestamp {
                    value.as_object_mut().unwrap().insert(
                        "ts_rfc3339".to_string(),
                        ts.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
                            .into(),
                    );
                }

                Some(value)
            })
            .collect::<Vec<_>>();

        let json = json!(ChronicleRequestBody {
            customer_id: self.customer_id.clone(),
            namespace: key.namespace,
            labels: self.labels.clone(),
            log_type: key.log_type,
            entries: events,
        });

        let size = as_tracked_write::<_, _, io::Error>(writer, &json, |writer, json| {
            serde_json::to_writer(writer, json)?;
            Ok(())
        })?;

        Ok((size, byte_size))
    }
}

// Settings required to produce a request that do not change per
// request. All possible values are pre-computed for direct use in
// producing a request.
#[derive(Clone, Debug)]
struct ChronicleRequestBuilder {
    encoder: ChronicleEncoder,
    compression: Compression,
}

struct ChronicleRequestPayload {
    bytes: Bytes,
}

impl From<Bytes> for ChronicleRequestPayload {
    fn from(bytes: Bytes) -> Self {
        Self { bytes }
    }
}

impl AsRef<[u8]> for ChronicleRequestPayload {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

impl RequestBuilder<(ChroniclePartitionKey, Vec<Event>)> for ChronicleRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = (ChroniclePartitionKey, Vec<Event>);
    type Encoder = ChronicleEncoder;
    type Payload = ChronicleRequestPayload;
    type Request = ChronicleRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (ChroniclePartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (partition_key, mut events) = input;
        let finalizers = events.take_finalizers();

        let builder = RequestMetadataBuilder::from_events(&events);
        (finalizers, builder, (partition_key, events))
    }

    fn build_request(
        &self,
        finalizers: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        ChronicleRequest {
            body: payload.into_payload().bytes,
            finalizers,
            metadata,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct Label {
    key: String,
    value: String,
}

impl ChronicleRequestBuilder {
    fn new(config: &ChronicleUnstructuredConfig) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.config().build()?;
        let encoder = crate::codecs::Encoder::<()>::new(serializer);
        let encoder = ChronicleEncoder {
            customer_id: config.customer_id.clone(),
            labels: config.labels.as_ref().map(|labs| {
                labs.iter()
                    .map(|(k, v)| Label {
                        key: k.to_string(),
                        value: v.to_string(),
                    })
                    .collect::<Vec<_>>()
            }),
            encoder,
            transformer,
        };
        let compression = Compression::from(config.compression);
        Ok(Self {
            encoder,
            compression,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ChronicleService {
    client: HttpClient,
    base_url: String,
    creds: GcpAuthenticator,
}

impl ChronicleService {
    pub const fn new(client: HttpClient, base_url: String, creds: GcpAuthenticator) -> Self {
        Self {
            client,
            base_url,
            creds,
        }
    }
}

#[derive(Debug, Snafu)]
pub enum ChronicleResponseError {
    #[snafu(display("Server responded with an error: {}", code))]
    ServerError { code: StatusCode },
    #[snafu(display("Failed to make HTTP(S) request: {}", error))]
    HttpError { error: crate::http::HttpError },
}

impl Service<ChronicleRequest> for ChronicleService {
    type Response = GcsResponse;
    type Error = ChronicleResponseError;
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

        let metadata = request.get_metadata().clone();

        let mut http_request = builder.body(Body::from(request.body)).unwrap();
        self.creds.apply(&mut http_request);

        let mut client = self.client.clone();
        Box::pin(async move {
            match client.call(http_request).await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        Ok(GcsResponse {
                            inner: response,
                            metadata,
                        })
                    } else {
                        Err(ChronicleResponseError::ServerError { code: status })
                    }
                }
                Err(error) => Err(ChronicleResponseError::HttpError { error }),
            }
        })
    }
}

#[cfg(all(test, feature = "chronicle-integration-tests"))]
mod integration_tests {
    use reqwest::{Client, Method, Response};
    use serde::{Deserialize, Serialize};
    use vector_lib::event::{BatchNotifier, BatchStatus};

    use super::*;
    use crate::test_util::{
        components::{
            run_and_assert_sink_compliance, run_and_assert_sink_error, COMPONENT_ERROR_TAGS,
            SINK_TAGS,
        },
        random_events_with_stream, random_string, trace_init,
    };

    const ADDRESS_ENV_VAR: &str = "CHRONICLE_ADDRESS";

    fn config(log_type: &str, auth_path: &str) -> ChronicleUnstructuredConfig {
        let address = std::env::var(ADDRESS_ENV_VAR).unwrap();
        let config = format!(
            indoc! { r#"
             endpoint = "{}"
             customer_id = "customer id"
             namespace = "namespace"
             credentials_path = "{}"
             log_type = "{}"
             encoding.codec = "text"
        "# },
            address, auth_path, log_type
        );

        let config: ChronicleUnstructuredConfig = toml::from_str(&config).unwrap();
        config
    }

    async fn config_build(
        log_type: &str,
        auth_path: &str,
    ) -> crate::Result<(VectorSink, crate::sinks::Healthcheck)> {
        let cx = SinkContext::default();
        config(log_type, auth_path).build(cx).await
    }

    #[tokio::test]
    async fn publish_events() {
        trace_init();

        let log_type = random_string(10);
        let (sink, healthcheck) =
            config_build(&log_type, "/home/vector/scripts/integration/gcp/auth.json")
                .await
                .expect("Building sink failed");

        healthcheck.await.expect("Health check failed");

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input, events) = random_events_with_stream(100, 100, Some(batch));
        run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let response = pull_messages(&log_type).await;
        let messages = response
            .into_iter()
            .map(|message| message.log_text)
            .collect::<Vec<_>>();
        assert_eq!(input.len(), messages.len());
        for i in 0..input.len() {
            let data = serde_json::to_value(&messages[i]).unwrap();
            let expected = serde_json::to_value(input[i].as_log().get("message").unwrap()).unwrap();
            assert_eq!(data, expected);
        }
    }

    #[tokio::test]
    async fn invalid_credentials() {
        trace_init();

        let log_type = random_string(10);
        // Test with an auth file that doesnt match the public key sent to the dummy chronicle server.
        let sink = config_build(
            &log_type,
            "/home/vector/scripts/integration/gcp/invalidauth.json",
        )
        .await;

        assert!(sink.is_err())
    }

    #[tokio::test]
    async fn publish_invalid_events() {
        trace_init();

        // The chronicle-emulator we are testing against is setup so a `log_type` of "INVALID"
        // will return a `400 BAD_REQUEST`.
        let log_type = "INVALID";
        let (sink, healthcheck) =
            config_build(log_type, "/home/vector/scripts/integration/gcp/auth.json")
                .await
                .expect("Building sink failed");

        healthcheck.await.expect("Health check failed");

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_input, events) = random_events_with_stream(100, 100, Some(batch));
        run_and_assert_sink_error(sink, events, &COMPONENT_ERROR_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
    }

    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct Log {
        customer_id: String,
        namespace: String,
        log_type: String,
        log_text: String,
        ts_rfc3339: String,
    }

    async fn request(method: Method, path: &str, log_type: &str) -> Response {
        let address = std::env::var(ADDRESS_ENV_VAR).unwrap();
        let url = format!("{}/{}", address, path);
        Client::new()
            .request(method.clone(), &url)
            .query(&[("log_type", log_type)])
            .send()
            .await
            .unwrap_or_else(|_| panic!("Sending {} request to {} failed", method, url))
    }

    async fn pull_messages(log_type: &str) -> Vec<Log> {
        request(Method::GET, "logs", log_type)
            .await
            .json::<Vec<Log>>()
            .await
            .expect("Extracting pull data failed")
    }
}
