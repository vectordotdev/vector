use std::{collections::HashMap, convert::TryFrom, io};

use bytes::Bytes;
use chrono::{FixedOffset, Utc};
use http::header::{HeaderName, HeaderValue};
use http::Uri;
use indoc::indoc;
use snafu::ResultExt;
use snafu::Snafu;
use tower::ServiceBuilder;
use uuid::Uuid;
use vector_lib::codecs::encoding::Framer;
use vector_lib::configurable::configurable_component;
use vector_lib::event::{EventFinalizers, Finalizable};
use vector_lib::{request_metadata::RequestMetadata, TimeZone};

use crate::sinks::util::metadata::RequestMetadataBuilder;
use crate::sinks::util::service::TowerRequestConfigDefaults;
use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType, Transformer},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    event::Event,
    gcp::{GcpAuthConfig, GcpAuthenticator, Scope},
    http::{get_http_scheme_from_uri, HttpClient},
    serde::json::to_string,
    sinks::{
        gcs_common::{
            config::{
                build_healthcheck, default_endpoint, GcsPredefinedAcl, GcsRetryLogic,
                GcsStorageClass,
            },
            service::{GcsRequest, GcsRequestSettings, GcsService},
            sink::GcsSink,
        },
        util::{
            batch::BatchConfig, partitioner::KeyPartitioner, request_builder::EncodeResult,
            timezone_to_offset, BulkSizeBasedDefaultBatchSettings, Compression, RequestBuilder,
            ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    template::{Template, TemplateParseError},
    tls::{TlsConfig, TlsSettings},
};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GcsHealthcheckError {
    #[snafu(display("key_prefix template parse error: {}", source))]
    KeyPrefixTemplate { source: TemplateParseError },
}

#[derive(Clone, Copy, Debug)]
pub struct GcsTowerRequestConfigDefaults;

impl TowerRequestConfigDefaults for GcsTowerRequestConfigDefaults {
    const RATE_LIMIT_NUM: u64 = 1_000;
}

/// Configuration for the `gcp_cloud_storage` sink.
#[configurable_component(sink(
    "gcp_cloud_storage",
    "Store observability events in GCP Cloud Storage."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct GcsSinkConfig {
    /// The GCS bucket name.
    #[configurable(metadata(docs::examples = "my-bucket"))]
    bucket: String,

    /// The Predefined ACL to apply to created objects.
    ///
    /// For more information, see [Predefined ACLs][predefined_acls].
    ///
    /// [predefined_acls]: https://cloud.google.com/storage/docs/access-control/lists#predefined-acl
    acl: Option<GcsPredefinedAcl>,

    /// The storage class for created objects.
    ///
    /// For more information, see the [storage classes][storage_classes] documentation.
    ///
    /// [storage_classes]: https://cloud.google.com/storage/docs/storage-classes
    storage_class: Option<GcsStorageClass>,

    /// The set of metadata `key:value` pairs for the created objects.
    ///
    /// For more information, see the [custom metadata][custom_metadata] documentation.
    ///
    /// [custom_metadata]: https://cloud.google.com/storage/docs/metadata#custom-metadata
    #[configurable(metadata(docs::additional_props_description = "A key/value pair."))]
    #[configurable(metadata(docs::advanced))]
    metadata: Option<HashMap<String, String>>,

    /// A prefix to apply to all object keys.
    ///
    /// Prefixes are useful for partitioning objects, such as by creating an object key that
    /// stores objects under a particular directory. If using a prefix for this purpose, it must end
    /// in `/` in order to act as a directory path. A trailing `/` is **not** automatically added.
    #[configurable(metadata(docs::templateable))]
    #[configurable(metadata(
        docs::examples = "date=%F/",
        docs::examples = "date=%F/hour=%H/",
        docs::examples = "year=%Y/month=%m/day=%d/",
        docs::examples = "application_id={{ application_id }}/date=%F/"
    ))]
    #[configurable(metadata(docs::advanced))]
    key_prefix: Option<String>,

    /// The timestamp format for the time component of the object key.
    ///
    /// By default, object keys are appended with a timestamp that reflects when the objects are
    /// sent to S3, such that the resulting object key is functionally equivalent to joining the key
    /// prefix with the formatted timestamp, such as `date=2022-07-18/1658176486`.
    ///
    /// This would represent a `key_prefix` set to `date=%F/` and the timestamp of Mon Jul 18 2022
    /// 20:34:44 GMT+0000, with the `filename_time_format` being set to `%s`, which renders
    /// timestamps in seconds since the Unix epoch.
    ///
    /// Supports the common [`strftime`][chrono_strftime_specifiers] specifiers found in most
    /// languages.
    ///
    /// When set to an empty string, no timestamp is appended to the key prefix.
    ///
    /// [chrono_strftime_specifiers]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
    #[serde(default = "default_time_format")]
    #[configurable(metadata(docs::advanced))]
    filename_time_format: String,

    /// Whether or not to append a UUID v4 token to the end of the object key.
    ///
    /// The UUID is appended to the timestamp portion of the object key, such that if the object key
    /// generated is `date=2022-07-18/1658176486`, setting this field to `true` results
    /// in an object key that looks like `date=2022-07-18/1658176486-30f6652c-71da-4f9f-800d-a1189c47c547`.
    ///
    /// This ensures there are no name collisions, and can be useful in high-volume workloads where
    /// object keys must be unique.
    #[serde(default = "crate::serde::default_true")]
    #[configurable(metadata(docs::advanced))]
    filename_append_uuid: bool,

    /// The filename extension to use in the object key.
    ///
    /// If not specified, the extension is determined by the compression scheme used.
    #[configurable(metadata(docs::advanced))]
    filename_extension: Option<String>,

    #[serde(flatten)]
    encoding: EncodingConfigWithFraming,

    #[configurable(derived)]
    #[serde(default)]
    compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    batch: BatchConfig<BulkSizeBasedDefaultBatchSettings>,

    /// API endpoint for Google Cloud Storage
    #[configurable(metadata(docs::examples = "http://localhost:9000"))]
    #[configurable(validation(format = "uri"))]
    #[serde(default = "default_endpoint")]
    endpoint: String,

    #[configurable(derived)]
    #[serde(default)]
    request: TowerRequestConfig<GcsTowerRequestConfigDefaults>,

    #[serde(flatten)]
    auth: GcpAuthConfig,

    #[configurable(derived)]
    tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub timezone: Option<TimeZone>,
}

fn default_time_format() -> String {
    "%s".to_string()
}

#[cfg(test)]
fn default_config(encoding: EncodingConfigWithFraming) -> GcsSinkConfig {
    GcsSinkConfig {
        bucket: Default::default(),
        acl: Default::default(),
        storage_class: Default::default(),
        metadata: Default::default(),
        key_prefix: Default::default(),
        filename_time_format: default_time_format(),
        filename_append_uuid: true,
        filename_extension: Default::default(),
        encoding,
        compression: Compression::gzip_default(),
        batch: Default::default(),
        endpoint: Default::default(),
        request: Default::default(),
        auth: Default::default(),
        tls: Default::default(),
        acknowledgements: Default::default(),
        timezone: Default::default(),
    }
}

impl GenerateConfig for GcsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            bucket = "my-bucket"
            credentials_path = "/path/to/credentials.json"
            framing.method = "newline_delimited"
            encoding.codec = "json"
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_cloud_storage")]
impl SinkConfig for GcsSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let auth = self.auth.build(Scope::DevStorageReadWrite).await?;
        let base_url = format!("{}/{}/", self.endpoint, self.bucket);
        let tls = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls, cx.proxy())?;
        let healthcheck = build_healthcheck(
            self.bucket.clone(),
            client.clone(),
            base_url.clone(),
            auth.clone(),
        )?;
        auth.spawn_regenerate_token();
        let sink = self.build_sink(client, base_url, auth, cx)?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl GcsSinkConfig {
    fn build_sink(
        &self,
        client: HttpClient,
        base_url: String,
        auth: GcpAuthenticator,
        cx: SinkContext,
    ) -> crate::Result<VectorSink> {
        let request = self.request.into_settings();

        let batch_settings = self.batch.into_batcher_settings()?;

        let partitioner = self.key_partitioner()?;

        let protocol = get_http_scheme_from_uri(&base_url.parse::<Uri>().unwrap());

        let svc = ServiceBuilder::new()
            .settings(request, GcsRetryLogic)
            .service(GcsService::new(client, base_url, auth));

        let request_settings = RequestSettings::new(self, cx)?;

        let sink = GcsSink::new(svc, request_settings, partitioner, batch_settings, protocol);

        Ok(VectorSink::from_event_streamsink(sink))
    }

    fn key_partitioner(&self) -> crate::Result<KeyPartitioner> {
        Ok(KeyPartitioner::new(
            Template::try_from(self.key_prefix.as_deref().unwrap_or("date=%F/"))
                .context(KeyPrefixTemplateSnafu)?,
        ))
    }
}

// Settings required to produce a request that do not change per
// request. All possible values are pre-computed for direct use in
// producing a request.
#[derive(Clone, Debug)]
struct RequestSettings {
    acl: Option<HeaderValue>,
    content_type: HeaderValue,
    content_encoding: Option<HeaderValue>,
    storage_class: HeaderValue,
    headers: Vec<(HeaderName, HeaderValue)>,
    extension: String,
    time_format: String,
    append_uuid: bool,
    encoder: (Transformer, Encoder<Framer>),
    compression: Compression,
    tz_offset: Option<FixedOffset>,
}

impl RequestBuilder<(String, Vec<Event>)> for RequestSettings {
    type Metadata = (String, EventFinalizers);
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = GcsRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (String, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (partition_key, mut events) = input;
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);

        ((partition_key, finalizers), builder, events)
    }

    fn build_request(
        &self,
        gcp_metadata: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (key, finalizers) = gcp_metadata;
        // TODO: pull the seconds from the last event
        let filename = {
            let seconds = match self.tz_offset {
                Some(offset) => Utc::now().with_timezone(&offset).format(&self.time_format),
                None => Utc::now()
                    .with_timezone(&chrono::Utc)
                    .format(&self.time_format),
            };

            if self.append_uuid {
                let uuid = Uuid::new_v4();
                format!("{}-{}", seconds, uuid.hyphenated())
            } else {
                seconds.to_string()
            }
        };

        let key = format!("{}{}.{}", key, filename, self.extension);
        let body = payload.into_payload();

        GcsRequest {
            key,
            body,
            finalizers,
            settings: GcsRequestSettings {
                acl: self.acl.clone(),
                content_type: self.content_type.clone(),
                content_encoding: self.content_encoding.clone(),
                storage_class: self.storage_class.clone(),
                headers: self.headers.clone(),
            },
            metadata,
        }
    }
}

impl RequestSettings {
    fn new(config: &GcsSinkConfig, cx: SinkContext) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();
        let (framer, serializer) = config.encoding.build(SinkType::MessageBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);
        let acl = config
            .acl
            .map(|acl| HeaderValue::from_str(&to_string(acl)).unwrap());
        let content_type = HeaderValue::from_str(encoder.content_type()).unwrap();
        let content_encoding = config
            .compression
            .content_encoding()
            .map(|ce| HeaderValue::from_str(&to_string(ce)).unwrap());
        let storage_class = config.storage_class.unwrap_or_default();
        let storage_class = HeaderValue::from_str(&to_string(storage_class)).unwrap();
        let metadata = config
            .metadata
            .as_ref()
            .map(|metadata| {
                metadata
                    .iter()
                    .map(make_header)
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_else(|| Ok(vec![]))?;
        let extension = config
            .filename_extension
            .clone()
            .unwrap_or_else(|| config.compression.extension().into());
        let time_format = config.filename_time_format.clone();
        let append_uuid = config.filename_append_uuid;
        let offset = config
            .timezone
            .or(cx.globals.timezone)
            .and_then(timezone_to_offset);

        Ok(Self {
            acl,
            content_type,
            content_encoding,
            storage_class,
            headers: metadata,
            extension,
            time_format,
            append_uuid,
            compression: config.compression,
            encoder: (transformer, encoder),
            tz_offset: offset,
        })
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
    use futures_util::{future::ready, stream};
    use vector_lib::codecs::encoding::FramingConfig;
    use vector_lib::codecs::{
        JsonSerializerConfig, NewlineDelimitedEncoderConfig, TextSerializerConfig,
    };
    use vector_lib::partition::Partitioner;
    use vector_lib::request_metadata::GroupedCountByteSize;
    use vector_lib::EstimatedJsonEncodedSizeOf;

    use crate::event::LogEvent;
    use crate::test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        http::{always_200_response, spawn_blackhole_http_server},
    };

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<GcsSinkConfig>();
    }

    #[tokio::test]
    async fn component_spec_compliance() {
        let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

        let context = SinkContext::default();

        let tls = TlsSettings::default();
        let client =
            HttpClient::new(tls, context.proxy()).expect("should not fail to create HTTP client");

        let config =
            default_config((None::<FramingConfig>, JsonSerializerConfig::default()).into());
        let sink = config
            .build_sink(
                client,
                mock_endpoint.to_string(),
                GcpAuthenticator::None,
                context,
            )
            .expect("failed to build sink");

        let event = Event::Log(LogEvent::from("simple message"));
        run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;
    }

    #[test]
    fn gcs_encode_event_apply_rules() {
        crate::test_util::trace_init();

        let message = "hello world".to_string();
        let mut event = LogEvent::from(message);
        event.insert("key", "value");

        let sink_config = GcsSinkConfig {
            key_prefix: Some("key: {{ key }}".into()),
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        };
        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&Event::Log(event))
            .expect("key wasn't provided");

        assert_eq!(key, "key: value");
    }

    fn request_settings(sink_config: &GcsSinkConfig, context: SinkContext) -> RequestSettings {
        RequestSettings::new(sink_config, context).expect("Could not create request settings")
    }

    fn build_request(extension: Option<&str>, uuid: bool, compression: Compression) -> GcsRequest {
        let context = SinkContext::default();
        let sink_config = GcsSinkConfig {
            key_prefix: Some("key/".into()),
            filename_time_format: "date".into(),
            filename_extension: extension.map(Into::into),
            filename_append_uuid: uuid,
            compression,
            ..default_config(
                (
                    Some(NewlineDelimitedEncoderConfig::new()),
                    JsonSerializerConfig::default(),
                )
                    .into(),
            )
        };
        let log = LogEvent::default().into();
        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&log)
            .expect("key wasn't provided");

        let mut byte_size = GroupedCountByteSize::new_untagged();
        byte_size.add_event(&log, log.estimated_json_encoded_size_of());

        let request_settings = request_settings(&sink_config, context);
        let (metadata, metadata_request_builder, _events) =
            request_settings.split_input((key, vec![log]));
        let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
        let request_metadata = metadata_request_builder.build(&payload);

        request_settings.build_request(metadata, request_metadata, payload)
    }

    #[test]
    fn gcs_build_request() {
        let req = build_request(Some("ext"), false, Compression::None);
        assert_eq!(req.key, "key/date.ext".to_string());

        let req = build_request(None, false, Compression::None);
        assert_eq!(req.key, "key/date.log".to_string());

        let req = build_request(None, false, Compression::gzip_default());
        assert_eq!(req.key, "key/date.log.gz".to_string());

        let req = build_request(None, true, Compression::gzip_default());
        assert_ne!(req.key, "key/date.log.gz".to_string());
    }
}
