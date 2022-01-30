use std::{
    collections::{BTreeMap, HashMap, HashSet},
    convert::TryFrom,
    io::{self, Write},
    num::NonZeroU64,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use azure_storage_blobs::prelude::ContainerClient;
use bytes::{BufMut, Bytes, BytesMut};
use chrono::{SecondsFormat, Utc};
use goauth::scopes::Scope;
use http::header::{HeaderName, HeaderValue};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use tower::ServiceBuilder;
use uuid::Uuid;
use vector_core::{
    config::{log_schema, LogSchema},
    event::{Event, Finalizable},
    ByteSizeOf,
};

use super::util::{
    batch::BatchError,
    encoding::{Encoder, StandardEncodings},
    BatchConfig, Compression, RequestBuilder, SinkBatchSettings,
};
use crate::{
    aws::{AwsAuthentication, RegionOrEndpoint},
    config::{DataType, GenerateConfig, SinkConfig, SinkContext},
    http::HttpClient,
    serde::to_string,
    sinks::{
        azure_common::{
            self,
            config::{AzureBlobMetadata, AzureBlobRequest, AzureBlobRetryLogic},
            service::AzureBlobService,
            sink::AzureBlobSink,
        },
        gcp::{GcpAuthConfig, GcpCredentials},
        gcs_common::{
            self,
            config::{GcsPredefinedAcl, GcsRetryLogic, GcsStorageClass, BASE_URL},
            service::{GcsMetadata, GcsRequest, GcsRequestSettings, GcsService},
            sink::GcsSink,
        },
        s3_common::{
            self,
            config::{
                create_service, S3CannedAcl, S3RetryLogic, S3ServerSideEncryption, S3StorageClass,
            },
            service::{S3Metadata, S3Request, S3Service},
            sink::S3Sink,
        },
        util::{partitioner::KeyPartitioner, ServiceBuilderExt, TowerRequestConfig},
        VectorSink,
    },
    template::Template,
    tls::{TlsOptions, TlsSettings},
};

const DEFAULT_COMPRESSION: Compression = Compression::gzip_default();

#[derive(Clone, Copy, Debug, Default)]
pub struct DatadogArchivesDefaultBatchSettings;

/// We should avoid producing many small batches - this might slow down Log Rehydration,
/// these values are similar with how DataDog's Log Archives work internally:
/// batch size - 100mb
/// batch timeout - 15min
impl SinkBatchSettings for DatadogArchivesDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(100_000_000);
    const TIMEOUT_SECS: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(900) };
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DatadogArchivesSinkConfig {
    pub service: String,
    pub bucket: String,
    pub key_prefix: Option<String>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(default)]
    pub aws_s3: Option<S3Config>,
    #[serde(default)]
    pub azure_blob: Option<AzureBlobConfig>,
    #[serde(default)]
    pub gcp_cloud_storage: Option<GcsConfig>,
    pub tls: Option<TlsOptions>,
    #[serde(default, skip_serializing)]
    batch: BatchConfig<DatadogArchivesDefaultBatchSettings>,
}

#[derive(Deserialize, Serialize, Default, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct S3Config {
    #[serde(flatten)]
    pub options: S3Options,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    #[serde(default)]
    pub auth: AwsAuthentication,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct S3Options {
    acl: Option<S3CannedAcl>,
    grant_full_control: Option<String>,
    grant_read: Option<String>,
    grant_read_acp: Option<String>,
    grant_write_acp: Option<String>,
    server_side_encryption: Option<S3ServerSideEncryption>,
    ssekms_key_id: Option<String>,
    storage_class: Option<S3StorageClass>,
    tags: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize, Serialize, Default, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct AzureBlobConfig {
    pub connection_string: String,
}

#[derive(Deserialize, Serialize, Default, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct GcsConfig {
    acl: Option<GcsPredefinedAcl>,
    storage_class: Option<GcsStorageClass>,
    metadata: Option<HashMap<String, String>>,
    #[serde(flatten)]
    auth: GcpAuthConfig,
}

impl GenerateConfig for DatadogArchivesSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            service: "".to_owned(),
            bucket: "".to_owned(),
            key_prefix: None,
            request: TowerRequestConfig::default(),
            aws_s3: None,
            gcp_cloud_storage: None,
            tls: None,
            azure_blob: None,
            batch: BatchConfig::default(),
        })
        .unwrap()
    }
}

#[derive(Debug, Snafu, PartialEq)]
enum ConfigError {
    #[snafu(display("Unsupported service: {}", service))]
    UnsupportedService { service: String },
    #[snafu(display("Unsupported storage class: {}", storage_class))]
    UnsupportedStorageClass { storage_class: String },
    #[snafu(display("Invalid batch configuration: {}", source))]
    InvalidBatchConfiguration { source: BatchError },
}

const KEY_TEMPLATE: &str = "/dt=%Y%m%d/hour=%H/";

impl DatadogArchivesSinkConfig {
    async fn build_sink(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        match &self.service[..] {
            "aws_s3" => {
                let s3_config = self.aws_s3.as_ref().expect("s3 config wasn't provided");
                let service = create_service(&s3_config.region, &s3_config.auth, None, &cx.proxy)?;
                let client = service.client();
                let svc = self
                    .build_s3_sink(&s3_config.options, service, cx)
                    .map_err(|error| format!("{}", error))?;
                Ok((
                    svc,
                    s3_common::config::build_healthcheck(self.bucket.clone(), client)?,
                ))
            }
            "azure_blob" => {
                let azure_config = self
                    .azure_blob
                    .as_ref()
                    .expect("azire blob config wasn't provided");
                let client = azure_common::config::build_client(
                    azure_config.connection_string.clone(),
                    self.bucket.clone(),
                )?;
                let svc = self
                    .build_azure_sink(Arc::<ContainerClient>::clone(&client), cx)
                    .map_err(|error| format!("{}", error))?;
                let healthcheck =
                    azure_common::config::build_healthcheck(self.bucket.clone(), client)?;
                Ok((svc, healthcheck))
            }
            "gcp_cloud_storage" => {
                let gcs_config = self
                    .gcp_cloud_storage
                    .as_ref()
                    .expect("gcs config wasn't provided");
                let creds = gcs_config
                    .auth
                    .make_credentials(Scope::DevStorageReadWrite)
                    .await?;
                let base_url = format!("{}{}/", BASE_URL, self.bucket);
                let tls = TlsSettings::from_options(&self.tls)?;
                let client = HttpClient::new(tls, cx.proxy())?;
                let healthcheck = gcs_common::config::build_healthcheck(
                    self.bucket.clone(),
                    client.clone(),
                    base_url.clone(),
                    creds.clone(),
                )?;
                let sink = self
                    .build_gcs_sink(client, base_url, creds, cx)
                    .map_err(|error| format!("{}", error))?;
                Ok((sink, healthcheck))
            }

            service => Err(Box::new(ConfigError::UnsupportedService {
                service: service.to_owned(),
            })),
        }
    }

    fn build_s3_sink(
        &self,
        s3_options: &S3Options,
        service: S3Service,
        cx: SinkContext,
    ) -> std::result::Result<VectorSink, ConfigError> {
        // we use lower default limits, because we send 100mb batches,
        // thus no need in the the higher number of outcoming requests
        let request_limits = self.request.unwrap_with(&Default::default());
        let service = ServiceBuilder::new()
            .settings(request_limits, S3RetryLogic)
            .service(service);

        match s3_options.storage_class {
            Some(class @ S3StorageClass::DeepArchive) | Some(class @ S3StorageClass::Glacier) => {
                return Err(ConfigError::UnsupportedStorageClass {
                    storage_class: format!("{:?}", class),
                });
            }
            _ => (),
        }

        let batcher_settings = self
            .batch
            .into_batcher_settings()
            .map_err(|source| ConfigError::InvalidBatchConfiguration { source })?;

        let partitioner = DatadogArchivesSinkConfig::build_partitioner();

        let s3_config = self
            .aws_s3
            .as_ref()
            .expect("s3 config wasn't provided")
            .clone();
        let request_builder =
            DatadogS3RequestBuilder::new(self.bucket.clone(), self.key_prefix.clone(), s3_config);

        let sink = S3Sink::new(cx, service, request_builder, partitioner, batcher_settings);

        Ok(VectorSink::from_event_streamsink(sink))
    }

    pub fn build_gcs_sink(
        &self,
        client: HttpClient,
        base_url: String,
        creds: Option<GcpCredentials>,
        cx: SinkContext,
    ) -> crate::Result<VectorSink> {
        let request = self.request.unwrap_with(&Default::default());

        let batcher_settings = self
            .batch
            .into_batcher_settings()
            .map_err(|source| ConfigError::InvalidBatchConfiguration { source })?;

        let svc = ServiceBuilder::new()
            .settings(request, GcsRetryLogic)
            .service(GcsService::new(client, base_url, creds));

        let gcs_config = self
            .gcp_cloud_storage
            .as_ref()
            .expect("gcs config wasn't provided")
            .clone();

        let acl = gcs_config
            .acl
            .map(|acl| HeaderValue::from_str(&to_string(acl)).unwrap());
        let storage_class = gcs_config.storage_class.unwrap_or_default();
        let storage_class = HeaderValue::from_str(&to_string(storage_class)).unwrap();
        let metadata = gcs_config
            .metadata
            .as_ref()
            .map(|metadata| {
                metadata
                    .iter()
                    .map(make_header)
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_else(|| Ok(vec![]))?;
        let request_builder = DatadogGcsRequestBuilder {
            bucket: self.bucket.clone(),
            key_prefix: self.key_prefix.clone(),
            acl,
            storage_class,
            metadata,
            encoding: DatadogArchivesEncoding::default(),
            compression: DEFAULT_COMPRESSION,
        };

        let partitioner = DatadogArchivesSinkConfig::build_partitioner();

        let sink = GcsSink::new(cx, svc, request_builder, partitioner, batcher_settings);

        Ok(VectorSink::from_event_streamsink(sink))
    }

    fn build_azure_sink(
        &self,
        client: Arc<ContainerClient>,
        cx: SinkContext,
    ) -> crate::Result<VectorSink> {
        let request_limits = self.request.unwrap_with(&Default::default());
        let service = ServiceBuilder::new()
            .settings(request_limits, AzureBlobRetryLogic)
            .service(AzureBlobService::new(client));

        let batcher_settings = self
            .batch
            .into_batcher_settings()
            .map_err(|source| ConfigError::InvalidBatchConfiguration { source })?;

        let partitioner = DatadogArchivesSinkConfig::build_partitioner();
        let request_builder = DatadogAzureRequestBuilder {
            container_name: self.bucket.clone(),
            blob_prefix: self.key_prefix.clone(),
            encoding: DatadogArchivesEncoding::default(),
        };

        let sink = AzureBlobSink::new(cx, service, request_builder, partitioner, batcher_settings);

        Ok(VectorSink::from_event_streamsink(sink))
    }

    pub fn build_partitioner() -> KeyPartitioner {
        KeyPartitioner::new(Template::try_from(KEY_TEMPLATE).expect("invalid object key format"))
    }
}

const RESERVED_ATTRIBUTES: [&str; 10] = [
    "_id", "date", "message", "host", "source", "service", "status", "tags", "trace_id", "span_id",
];

#[derive(Debug)]
struct DatadogArchivesEncoding {
    inner: StandardEncodings,
    reserved_attributes: HashSet<&'static str>,
    id_rnd_bytes: [u8; 8],
    id_seq_number: AtomicU32,
    log_schema: &'static LogSchema,
}

impl DatadogArchivesEncoding {
    /// Generates a unique event ID compatible with DD:
    /// - 18 bytes;
    /// - first 6 bytes represent a "now" timestamp in millis;
    /// - the rest 12 bytes can be just any sequence unique for a given timestamp.
    ///
    /// To generate unique-ish trailing 12 bytes we use random 8 bytes, generated at startup,
    /// and a rolling-over 4-bytes sequence number.
    fn generate_log_id(&self) -> String {
        let mut id = BytesMut::with_capacity(18);
        // timestamp in millis - 6 bytes
        let now = Utc::now();
        id.put_int(now.timestamp_millis(), 6);

        // 8 random bytes
        id.put_slice(&self.id_rnd_bytes);

        // 4 bytes for the counter should be more than enough - it should be unique for 1 millisecond only
        let id_seq_number = self.id_seq_number.fetch_add(1, Ordering::Relaxed);
        id.put_u32(id_seq_number);

        base64::encode(id.freeze())
    }
}

impl Default for DatadogArchivesEncoding {
    fn default() -> Self {
        Self {
            inner: StandardEncodings::Ndjson,
            reserved_attributes: RESERVED_ATTRIBUTES.to_vec().into_iter().collect(),
            id_rnd_bytes: thread_rng().gen::<[u8; 8]>(),
            id_seq_number: AtomicU32::new(0),
            log_schema: log_schema(),
        }
    }
}

impl Encoder<Vec<Event>> for DatadogArchivesEncoding {
    /// Applies the following transformations to align event's schema with DD:
    /// - `_id` is generated in the sink(format described below);
    /// - `date` is set from the Global Log Schema's `timestamp` mapping, or to the current time if missing;
    /// - `message`,`host` are set from the corresponding Global Log Schema mappings;
    /// - `source`, `service`, `status`, `tags` and other reserved attributes are left as is;
    /// - the rest of the fields is moved to `attributes`.
    fn encode_input(&self, mut input: Vec<Event>, writer: &mut dyn Write) -> io::Result<usize> {
        for event in input.iter_mut() {
            let log_event = event.as_mut_log();

            log_event.insert("_id", self.generate_log_id());

            let timestamp = log_event
                .remove(self.log_schema.timestamp_key())
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis().into());
            log_event.insert(
                "date",
                timestamp
                    .as_timestamp()
                    .cloned()
                    .unwrap_or_else(chrono::Utc::now)
                    .to_rfc3339_opts(SecondsFormat::Millis, true),
            );
            log_event.rename_key_flat(self.log_schema.message_key(), "message");
            log_event.rename_key_flat(self.log_schema.host_key(), "host");

            let mut attributes = BTreeMap::new();
            let custom_attributes: Vec<String> = log_event
                .as_map()
                .keys()
                .filter(|&path| !self.reserved_attributes.contains(path.as_str()))
                .map(|v| v.to_owned())
                .collect();
            for path in custom_attributes {
                if let Some(value) = log_event.remove(&path) {
                    attributes.insert(path, value);
                }
            }
            log_event.insert("attributes", attributes);
        }

        self.inner.encode_input(input, writer)
    }
}
#[derive(Debug)]
struct DatadogS3RequestBuilder {
    bucket: String,
    key_prefix: Option<String>,
    config: S3Config,
    encoding: DatadogArchivesEncoding,
}

impl DatadogS3RequestBuilder {
    pub fn new(bucket: String, key_prefix: Option<String>, config: S3Config) -> Self {
        Self {
            bucket,
            key_prefix,
            config,
            encoding: DatadogArchivesEncoding::default(),
        }
    }
}

impl RequestBuilder<(String, Vec<Event>)> for DatadogS3RequestBuilder {
    type Metadata = S3Metadata;
    type Events = Vec<Event>;
    type Encoder = DatadogArchivesEncoding;
    type Payload = Bytes;
    type Request = S3Request;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        DEFAULT_COMPRESSION
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }

    fn split_input(&self, input: (String, Vec<Event>)) -> (Self::Metadata, Self::Events) {
        let (partition_key, mut events) = input;
        let finalizers = events.take_finalizers();
        let metadata = S3Metadata {
            partition_key,
            count: events.len(),
            byte_size: events.size_of(),
            finalizers,
        };

        (metadata, events)
    }

    fn build_request(&self, mut metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        metadata.partition_key =
            generate_object_key(self.key_prefix.clone(), metadata.partition_key);

        trace!(
            message = "Sending events.",
            bytes = ?payload.len(),
            events_len = ?metadata.byte_size,
            bucket = ?self.bucket,
            key = ?metadata.partition_key
        );

        let s3_options = self.config.options.clone();
        S3Request {
            body: payload,
            bucket: self.bucket.clone(),
            metadata,
            content_encoding: DEFAULT_COMPRESSION.content_encoding(),
            options: s3_common::config::S3Options {
                acl: s3_options.acl,
                grant_full_control: s3_options.grant_full_control,
                grant_read: s3_options.grant_read,
                grant_read_acp: s3_options.grant_read_acp,
                grant_write_acp: s3_options.grant_write_acp,
                server_side_encryption: s3_options.server_side_encryption,
                ssekms_key_id: s3_options.ssekms_key_id,
                storage_class: s3_options.storage_class,
                tags: s3_options.tags,
                content_encoding: None,
                content_type: None,
            },
        }
    }
}

#[derive(Debug)]
struct DatadogGcsRequestBuilder {
    bucket: String,
    key_prefix: Option<String>,
    acl: Option<HeaderValue>,
    storage_class: HeaderValue,
    metadata: Vec<(HeaderName, HeaderValue)>,
    encoding: DatadogArchivesEncoding,
    compression: Compression,
}

impl RequestBuilder<(String, Vec<Event>)> for DatadogGcsRequestBuilder {
    type Metadata = GcsMetadata;
    type Events = Vec<Event>;
    type Payload = Bytes;
    type Request = GcsRequest;
    type Encoder = DatadogArchivesEncoding;
    type Error = io::Error;

    fn split_input(&self, input: (String, Vec<Event>)) -> (Self::Metadata, Self::Events) {
        let (key, mut events) = input;
        let finalizers = events.take_finalizers();
        let metadata = GcsMetadata {
            key,
            count: events.len(),
            byte_size: events.size_of(),
            finalizers,
        };

        (metadata, events)
    }

    fn build_request(&self, mut metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        metadata.key = generate_object_key(self.key_prefix.clone(), metadata.key);

        trace!(
            message = "Sending events.",
            bytes = ?payload.len(),
            events_len = ?metadata.count,
            bucket = ?self.bucket,
            key = ?metadata.key
        );

        let content_type = HeaderValue::from_str(StandardEncodings::Ndjson.content_type()).unwrap();
        let content_encoding = DEFAULT_COMPRESSION
            .content_encoding()
            .map(|ce| HeaderValue::from_str(&to_string(ce)).unwrap());

        GcsRequest {
            body: payload,
            settings: GcsRequestSettings {
                acl: self.acl.clone(),
                content_type,
                content_encoding,
                storage_class: self.storage_class.clone(),
                headers: self.metadata.clone(),
            },
            metadata,
        }
    }

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }
}

fn generate_object_key(key_prefix: Option<String>, partition_key: String) -> String {
    let filename = Uuid::new_v4().to_string();

    let key = format!(
        "{}/{}{}.{}",
        key_prefix.unwrap_or_default(),
        partition_key,
        filename,
        "json.gz"
    )
    .replace("//", "/");
    key
}

#[derive(Debug)]
struct DatadogAzureRequestBuilder {
    container_name: String,
    blob_prefix: Option<String>,
    encoding: DatadogArchivesEncoding,
}

impl RequestBuilder<(String, Vec<Event>)> for DatadogAzureRequestBuilder {
    type Metadata = AzureBlobMetadata;
    type Events = Vec<Event>;
    type Encoder = DatadogArchivesEncoding;
    type Payload = Bytes;
    type Request = AzureBlobRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        DEFAULT_COMPRESSION
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }

    fn split_input(&self, input: (String, Vec<Event>)) -> (Self::Metadata, Self::Events) {
        let (partition_key, mut events) = input;
        let finalizers = events.take_finalizers();
        let metadata = AzureBlobMetadata {
            partition_key,
            count: events.len(),
            byte_size: events.size_of(),
            finalizers,
        };

        (metadata, events)
    }

    fn build_request(&self, mut metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        metadata.partition_key =
            generate_object_key(self.blob_prefix.clone(), metadata.partition_key);

        trace!(
            message = "Sending events.",
            bytes = ?payload.len(),
            events_len = ?metadata.count,
            container = ?self.container_name,
            blob = ?metadata.partition_key
        );

        AzureBlobRequest {
            blob_data: payload,
            content_encoding: DEFAULT_COMPRESSION.content_encoding(),
            content_type: "application/gzip",
            metadata,
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_archives")]
impl SinkConfig for DatadogArchivesSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink_and_healthcheck = self.build_sink(cx).await?;
        Ok(sink_and_healthcheck)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "datadog_archives"
    }
}

// Make a header pair from a key-value string pair
fn make_header((name, value): (&String, &String)) -> crate::Result<(HeaderName, HeaderValue)> {
    Ok((
        HeaderName::from_bytes(name.as_bytes())?,
        HeaderValue::from_str(value)?,
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::print_stdout)] // tests

    use std::{collections::BTreeMap, io::Cursor};

    use chrono::DateTime;
    use vector_core::partition::Partitioner;

    use super::*;
    use crate::event::LogEvent;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogArchivesSinkConfig>();
    }

    #[test]
    fn encodes_event() {
        let mut event = Event::from("test message");
        let log_mut = event.as_mut_log();
        log_mut.insert("service", "test-service");
        log_mut.insert("not_a_reserved_attribute", "value");
        log_mut.insert("tags", vec!["tag1:value1", "tag2:value2"]);
        let timestamp = DateTime::parse_from_rfc3339("2021-08-23T18:00:27.879+02:00")
            .expect("invalid test case")
            .with_timezone(&Utc);
        log_mut.insert("timestamp", timestamp);

        let mut writer = Cursor::new(Vec::new());
        let encoding = DatadogArchivesEncoding::default();
        let _ = encoding.encode_input(vec![event], &mut writer);

        let encoded = writer.into_inner();
        let json: BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(encoded.as_slice()).unwrap();

        validate_event_id(
            json.get("_id")
                .expect("_id not found")
                .as_str()
                .expect("_id is not a string"),
        );

        assert_eq!(json.len(), 6); // _id, message, date, service, attributes
        assert_eq!(
            json.get("message")
                .expect("message not found")
                .as_str()
                .expect("message is not a string"),
            "test message"
        );
        assert_eq!(
            json.get("date")
                .expect("date not found")
                .as_str()
                .expect("date is not a string"),
            "2021-08-23T16:00:27.879Z"
        );
        assert_eq!(
            json.get("service")
                .expect("service not found")
                .as_str()
                .expect("service is not a string"),
            "test-service"
        );

        assert_eq!(
            json.get("tags")
                .expect("tags not found")
                .as_array()
                .expect("service is not an array")
                .to_owned(),
            vec!["tag1:value1", "tag2:value2"]
        );

        let attributes = json
            .get("attributes")
            .expect("attributes not found")
            .as_object()
            .expect("attributes is not an object");
        assert_eq!(attributes.len(), 1);
        assert_eq!(
            String::from_utf8_lossy(
                attributes
                    .get("not_a_reserved_attribute")
                    .expect("not_a_reserved_attribute wasn't moved to attributes")
                    .as_str()
                    .expect("not_a_reserved_attribute is not a string")
                    .as_ref()
            ),
            "value"
        );
    }

    #[test]
    fn generates_valid_key_for_an_event() {
        let mut log = LogEvent::from("test message");

        let timestamp = DateTime::parse_from_rfc3339("2021-08-23T18:00:27.879+02:00")
            .expect("invalid test case")
            .with_timezone(&Utc);
        log.insert("timestamp", timestamp);

        let partitioner = DatadogArchivesSinkConfig::build_partitioner();
        let key = partitioner
            .partition(&log.into())
            .expect("key wasn't provided");

        assert_eq!(key, "/dt=20210823/hour=16/");
    }

    #[test]
    fn generates_valid_id() {
        let log1 = Event::from("test event 1");
        let mut writer = Cursor::new(Vec::new());
        let encoding = DatadogArchivesEncoding::default();
        let _ = encoding.encode_input(vec![log1], &mut writer);
        let encoded = writer.into_inner();
        let json: BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(encoded.as_slice()).unwrap();
        let id1 = json
            .get("_id")
            .expect("_id not found")
            .as_str()
            .expect("_id is not a string");
        validate_event_id(id1);

        // check that id is different for the next event
        let log2 = Event::from("test event 2");
        let mut writer = Cursor::new(Vec::new());
        let _ = encoding.encode_input(vec![log2], &mut writer);
        let encoded = writer.into_inner();
        let json: BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(encoded.as_slice()).unwrap();
        let id2 = json
            .get("_id")
            .expect("_id not found")
            .as_str()
            .expect("_id is not a string");
        validate_event_id(id2);
        assert_ne!(id1, id2)
    }

    #[test]
    fn generates_date_if_missing() {
        let log = Event::from("test message");
        let mut writer = Cursor::new(Vec::new());
        let encoding = DatadogArchivesEncoding::default();
        let _ = encoding.encode_input(vec![log], &mut writer);
        let encoded = writer.into_inner();
        let json: BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(encoded.as_slice()).unwrap();

        let date = DateTime::parse_from_rfc3339(
            json.get("date")
                .expect("date not found")
                .as_str()
                .expect("date is not a string"),
        )
        .expect("date is not in an rfc3339 format");

        // check that it is a recent timestamp
        assert!(Utc::now().timestamp() - date.timestamp() < 1000);
    }

    /// check that _id is:
    /// - 18 bytes,
    /// - base64-encoded,
    /// - first 6 bytes - a "now" timestamp in millis
    fn validate_event_id(id: &str) {
        let bytes = base64::decode(id).expect("_id is not base64-encoded");
        assert_eq!(bytes.len(), 18);
        let mut timestamp: [u8; 8] = [0; 8];
        for (i, b) in bytes[..6].iter().enumerate() {
            timestamp[i + 2] = *b;
        }
        let timestamp = i64::from_be_bytes(timestamp);
        // check that it is a recent timestamp in millis
        assert!(Utc::now().timestamp_millis() - timestamp < 1000);
    }

    #[test]
    fn s3_build_request() {
        let fake_buf = Bytes::new();
        let mut log = Event::from("test message");
        let timestamp = DateTime::parse_from_rfc3339("2021-08-23T18:00:27.879+02:00")
            .expect("invalid test case")
            .with_timezone(&Utc);
        log.as_mut_log().insert("timestamp", timestamp);
        let partitioner = DatadogArchivesSinkConfig::build_partitioner();
        let key = partitioner.partition(&log).expect("key wasn't provided");

        let request_builder = DatadogS3RequestBuilder::new(
            "dd-logs".into(),
            Some("audit".into()),
            S3Config::default(),
        );

        let (metadata, _events) = request_builder.split_input((key, vec![log]));
        let req = request_builder.build_request(metadata, fake_buf.clone());
        let expected_key_prefix = "audit/dt=20210823/hour=16/";
        let expected_key_ext = ".json.gz";
        println!("{}", req.metadata.partition_key);
        assert!(req.metadata.partition_key.starts_with(expected_key_prefix));
        assert!(req.metadata.partition_key.ends_with(expected_key_ext));
        let uuid1 = &req.metadata.partition_key
            [expected_key_prefix.len()..req.metadata.partition_key.len() - expected_key_ext.len()];
        assert_eq!(uuid1.len(), 36);

        // check the the second batch has a different UUID
        let log2 = Event::new_empty_log();

        let key = partitioner.partition(&log2).expect("key wasn't provided");
        let (metadata, _events) = request_builder.split_input((key, vec![log2]));
        let req = request_builder.build_request(metadata, fake_buf);
        let uuid2 = &req.metadata.partition_key
            [expected_key_prefix.len()..req.metadata.partition_key.len() - expected_key_ext.len()];
        assert_ne!(uuid1, uuid2);
    }

    #[tokio::test]
    async fn error_if_unsupported_s3_storage_class() {
        for (class, supported) in [
            (S3StorageClass::Standard, true),
            (S3StorageClass::StandardIa, true),
            (S3StorageClass::IntelligentTiering, true),
            (S3StorageClass::OnezoneIa, true),
            (S3StorageClass::ReducedRedundancy, true),
            (S3StorageClass::DeepArchive, false),
            (S3StorageClass::Glacier, false),
        ] {
            let config = DatadogArchivesSinkConfig {
                service: "aws_s3".to_owned(),
                bucket: "vector-datadog-archives".to_owned(),
                key_prefix: Some("logs/".to_owned()),
                request: TowerRequestConfig::default(),
                aws_s3: Some(S3Config {
                    options: S3Options {
                        storage_class: Some(class),
                        ..Default::default()
                    },
                    region: RegionOrEndpoint::with_region("us-east-1".to_owned()),
                    auth: Default::default(),
                }),
                azure_blob: None,
                gcp_cloud_storage: None,
                tls: None,
                batch: BatchConfig::default(),
            };

            let res = config.build_sink(SinkContext::new_test()).await;

            if supported {
                assert!(res.is_ok());
            } else {
                assert_eq!(
                    res.err().unwrap().to_string(),
                    format!(r#"Unsupported storage class: {:?}"#, class)
                );
            }
        }
    }
}
