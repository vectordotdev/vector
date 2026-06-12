use std::{collections::HashMap, convert::TryFrom, io};

use bytes::Bytes;
use chrono::{FixedOffset, Utc};
use http::{
    Uri,
    header::{HeaderName, HeaderValue},
};
use indoc::indoc;
use snafu::{ResultExt, Snafu};
use tower::ServiceBuilder;
use uuid::Uuid;
#[cfg(feature = "codecs-parquet")]
use vector_lib::codecs::BatchEncoder;
#[cfg(feature = "codecs-parquet")]
use vector_lib::codecs::encoding::{BatchSerializerConfig, format::ParquetSerializerConfig};
use vector_lib::{
    TimeZone,
    codecs::{EncoderKind, encoding::Framer},
    configurable::configurable_component,
    event::{EventFinalizers, Finalizable},
    request_metadata::RequestMetadata,
};

use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType, Transformer},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    event::Event,
    gcp::{GcpAuthConfig, GcpAuthenticator, Scope},
    http::{HttpClient, get_http_scheme_from_uri},
    serde::json::to_string,
    sinks::{
        Healthcheck, VectorSink,
        gcs_common::{
            config::{
                GcsPredefinedAcl, GcsRetryLogic, GcsStorageClass, build_healthcheck,
                default_endpoint,
            },
            service::{GcsRequest, GcsRequestSettings, GcsService},
            sink::GcsSink,
        },
        util::{
            BulkSizeBasedDefaultBatchSettings, Compression, RequestBuilder, ServiceBuilderExt,
            TowerRequestConfig, batch::BatchConfig, metadata::RequestMetadataBuilder,
            partitioner::KeyPartitioner, request_builder::EncodeResult,
            service::TowerRequestConfigDefaults, timezone_to_offset,
        },
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

/// Batch encoding configuration for the `gcp_cloud_storage` sink.
#[cfg(feature = "codecs-parquet")]
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "codec", rename_all = "snake_case")]
#[configurable(metadata(
    docs::enum_tag_description = "The codec to use for batch encoding events."
))]
pub enum GcsBatchEncoding {
    /// Encodes events in [Apache Parquet][apache_parquet] columnar format.
    ///
    /// [apache_parquet]: https://parquet.apache.org/
    Parquet(ParquetSerializerConfig),
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

    /// Batch encoding configuration for columnar formats.
    ///
    /// When set, events are encoded together as a batch in a columnar format (Parquet)
    /// instead of the standard per-event framing-based encoding. The columnar format handles
    /// its own internal compression, so the top-level `compression` setting is bypassed.
    #[cfg(feature = "codecs-parquet")]
    #[configurable(derived)]
    #[serde(default)]
    batch_encoding: Option<GcsBatchEncoding>,

    /// Compression configuration.
    ///
    /// All compression algorithms use the default compression level unless otherwise specified.
    ///
    /// Some cloud storage API clients and browsers handle decompression transparently, so
    /// depending on how they are accessed, files may not always appear to be compressed.
    #[configurable(derived)]
    #[serde(default)]
    compression: Compression,

    /// Overrides the MIME type of the created objects.
    ///
    /// Directly comparable to the `Content-Type` HTTP header.
    ///
    /// If not specified, defaults to the encoder's content type.
    #[configurable(metadata(
        docs::examples = "text/plain; charset=utf-8",
        docs::examples = "application/gzip"
    ))]
    content_type: Option<String>,

    /// Overrides what content encoding has been applied to the object.
    ///
    /// Directly comparable to the `Content-Encoding` HTTP header.
    ///
    /// If not specified, the compression scheme used dictates this value.
    #[configurable(metadata(docs::examples = "gzip", docs::examples = "zstd"))]
    content_encoding: Option<String>,

    /// Sets the `Cache-Control` header for the created objects.
    ///
    /// Directly comparable to the `Cache-Control` HTTP header.
    #[configurable(metadata(docs::examples = "no-transform"))]
    cache_control: Option<String>,

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
        content_type: Default::default(),
        content_encoding: Default::default(),
        cache_control: Default::default(),
        encoding,
        #[cfg(feature = "codecs-parquet")]
        batch_encoding: Default::default(),
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
        let tls = TlsSettings::from_options(self.tls.as_ref())?;
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
            .settings(request, GcsRetryLogic::default())
            .service(GcsService::new(client, base_url, auth));

        let request_settings = RequestSettings::new(self, cx)?;

        let sink = GcsSink::new(svc, request_settings, partitioner, batch_settings, protocol);

        Ok(VectorSink::from_event_streamsink(sink))
    }

    fn key_partitioner(&self) -> crate::Result<KeyPartitioner> {
        Ok(KeyPartitioner::new(
            Template::try_from(self.key_prefix.as_deref().unwrap_or("date=%F/"))
                .context(KeyPrefixTemplateSnafu)?,
            None,
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
    cache_control: Option<HeaderValue>,
    headers: Vec<(HeaderName, HeaderValue)>,
    extension: String,
    time_format: String,
    append_uuid: bool,
    encoder: (Transformer, EncoderKind),
    compression: Compression,
    tz_offset: Option<FixedOffset>,
}

impl RequestBuilder<(String, Vec<Event>)> for RequestSettings {
    type Metadata = (String, EventFinalizers);
    type Events = Vec<Event>;
    type Encoder = (Transformer, EncoderKind);
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
                cache_control: self.cache_control.clone(),
                headers: self.headers.clone(),
            },
            metadata,
        }
    }
}

impl RequestSettings {
    fn new(config: &GcsSinkConfig, cx: SinkContext) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();

        // Determine the encoder, the effective compression, the derived
        // content type, and the default filename extension. When a columnar
        // `batch_encoding` (e.g. Parquet) is configured, events are encoded as
        // a batch and the format handles its own compression internally, so the
        // top-level `compression` setting is bypassed.
        let (encoder, effective_compression, derived_content_type, default_extension): (
            EncoderKind,
            Compression,
            String,
            Option<String>,
        ) = {
            #[cfg(feature = "codecs-parquet")]
            if let Some(batch_encoding) = &config.batch_encoding {
                let GcsBatchEncoding::Parquet(parquet_config) = batch_encoding;
                let resolved_batch_config = BatchSerializerConfig::Parquet(parquet_config.clone());
                let batch_serializer = resolved_batch_config.build_batch_serializer()?;
                let batch_encoder = BatchEncoder::new(batch_serializer);
                let content_type = batch_encoder
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();

                if config.compression != Compression::None {
                    warn!(
                        "Top level compression setting ignored when batch_encoding set to parquet."
                    );
                }

                (
                    EncoderKind::Batch(batch_encoder),
                    Compression::None,
                    content_type,
                    Some("parquet".to_string()),
                )
            } else {
                let (framer, serializer) = config.encoding.build(SinkType::MessageBased)?;
                let encoder = Encoder::<Framer>::new(framer, serializer);
                let content_type = encoder.content_type().to_string();
                (
                    EncoderKind::Framed(Box::new(encoder)),
                    config.compression,
                    content_type,
                    None,
                )
            }

            #[cfg(not(feature = "codecs-parquet"))]
            {
                let (framer, serializer) = config.encoding.build(SinkType::MessageBased)?;
                let encoder = Encoder::<Framer>::new(framer, serializer);
                let content_type = encoder.content_type().to_string();
                (
                    EncoderKind::Framed(Box::new(encoder)),
                    config.compression,
                    content_type,
                    None,
                )
            }
        };

        let acl = config
            .acl
            .map(|acl| HeaderValue::from_str(&to_string(acl)).unwrap());
        let content_type_str = config.content_type.clone().unwrap_or(derived_content_type);
        let content_type = HeaderValue::from_str(&content_type_str)?;
        let content_encoding = match &config.content_encoding {
            Some(ce) => Some(HeaderValue::from_str(ce)?),
            None => effective_compression
                .content_encoding()
                .map(|ce| HeaderValue::from_str(&to_string(ce)).unwrap()),
        };
        let storage_class = config.storage_class.unwrap_or_default();
        let storage_class = HeaderValue::from_str(&to_string(storage_class)).unwrap();
        let cache_control = config
            .cache_control
            .as_ref()
            .map(|cc| HeaderValue::from_str(cc))
            .transpose()?;
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
            .or(default_extension)
            .unwrap_or_else(|| effective_compression.extension().into());
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
            cache_control,
            headers: metadata,
            extension,
            time_format,
            append_uuid,
            compression: effective_compression,
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
    use vector_lib::{
        EstimatedJsonEncodedSizeOf,
        codecs::{
            JsonSerializerConfig, NewlineDelimitedEncoderConfig, TextSerializerConfig,
            encoding::FramingConfig,
        },
        partition::Partitioner,
        request_metadata::GroupedCountByteSize,
    };

    use super::*;
    use crate::{
        event::LogEvent,
        test_util::{
            components::{SINK_TAGS, run_and_assert_sink_compliance},
            http::{always_200_response, spawn_blackhole_http_server},
        },
    };

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

    #[test]
    fn gcs_content_type_default() {
        let context = SinkContext::default();
        let sink_config = GcsSinkConfig {
            content_type: None,
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        };

        let request_settings = request_settings(&sink_config, context);
        // Should default to encoder's content type which is "text/plain" for text codec
        assert_eq!(
            request_settings.content_type.to_str().unwrap(),
            "text/plain"
        );
    }

    #[test]
    fn gcs_content_type_custom() {
        let context = SinkContext::default();
        let sink_config = GcsSinkConfig {
            content_type: Some("text/plain; charset=utf-8".to_string()),
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        };

        let request_settings = request_settings(&sink_config, context);
        // Should use custom content type
        assert_eq!(
            request_settings.content_type.to_str().unwrap(),
            "text/plain; charset=utf-8"
        );
    }

    #[test]
    fn gcs_content_type_invalid() {
        let context = SinkContext::default();
        let sink_config = GcsSinkConfig {
            // Invalid header value with newline character
            content_type: Some("text/plain\nInvalid".to_string()),
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        };

        let result = RequestSettings::new(&sink_config, context);
        // Should return an error, not panic
        assert!(result.is_err());
    }

    #[test]
    fn gcs_content_encoding_default() {
        let context = SinkContext::default();
        let sink_config = GcsSinkConfig {
            content_encoding: None,
            compression: Compression::gzip_default(),
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        };

        let request_settings = request_settings(&sink_config, context);
        // Should default to compression's content encoding which is "gzip"
        assert_eq!(
            request_settings.content_encoding.unwrap().to_str().unwrap(),
            "gzip"
        );
    }

    #[test]
    fn gcs_content_encoding_none_when_no_compression() {
        let context = SinkContext::default();
        let sink_config = GcsSinkConfig {
            content_encoding: None,
            compression: Compression::None,
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        };

        let request_settings = request_settings(&sink_config, context);
        // Should be None when compression is None
        assert!(request_settings.content_encoding.is_none());
    }

    #[test]
    fn gcs_content_encoding_custom() {
        let context = SinkContext::default();
        let sink_config = GcsSinkConfig {
            content_encoding: Some("gzip".to_string()),
            compression: Compression::None,
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        };

        let request_settings = request_settings(&sink_config, context);
        // Should use custom content encoding
        assert_eq!(
            request_settings.content_encoding.unwrap().to_str().unwrap(),
            "gzip"
        );
    }

    #[test]
    fn gcs_content_encoding_invalid() {
        let context = SinkContext::default();
        let sink_config = GcsSinkConfig {
            // Invalid header value with newline character
            content_encoding: Some("gzip\nInvalid".to_string()),
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        };

        let result = RequestSettings::new(&sink_config, context);
        // Should return an error, not panic
        assert!(result.is_err());
    }

    #[test]
    fn gcs_content_encoding_empty() {
        let context = SinkContext::default();
        let sink_config = GcsSinkConfig {
            // Empty string to disable content encoding header even with compression
            content_encoding: Some("".to_string()),
            compression: Compression::gzip_default(),
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        };

        let request_settings = request_settings(&sink_config, context);
        // Should use empty content encoding (overriding the compression default)
        assert_eq!(
            request_settings.content_encoding.unwrap().to_str().unwrap(),
            ""
        );
    }

    #[test]
    fn gcs_cache_control_default() {
        let context = SinkContext::default();
        let sink_config = GcsSinkConfig {
            cache_control: None,
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        };

        let request_settings = request_settings(&sink_config, context);
        // Should be None by default
        assert!(request_settings.cache_control.is_none());
    }

    #[test]
    fn gcs_cache_control_custom() {
        let context = SinkContext::default();
        let sink_config = GcsSinkConfig {
            cache_control: Some("no-transform".to_string()),
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        };

        let request_settings = request_settings(&sink_config, context);
        assert_eq!(
            request_settings.cache_control.unwrap().to_str().unwrap(),
            "no-transform"
        );
    }

    #[test]
    fn gcs_cache_control_invalid() {
        let context = SinkContext::default();
        let sink_config = GcsSinkConfig {
            // Invalid header value with newline character
            cache_control: Some("no-cache\nInvalid".to_string()),
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        };

        let result = RequestSettings::new(&sink_config, context);
        // Should return an error, not panic
        assert!(result.is_err());
    }

    /// Correct TOML shape: `batch_encoding.codec = "parquet"` with `schema_mode = "auto_infer"`.
    #[cfg(feature = "codecs-parquet")]
    #[test]
    fn parquet_batch_encoding_correct_toml_shape() {
        use vector_lib::codecs::encoding::format::{ParquetCompression, ParquetSchemaMode};

        let config: GcsSinkConfig = toml::from_str(
            r#"
            bucket = "test-bucket"
            compression = "none"

            [encoding]
            codec = "text"

            [batch_encoding]
            schema_mode = "auto_infer"
            codec = "parquet"

            [batch_encoding.compression]
            algorithm = "snappy"
            "#,
        )
        .expect("correct batch_encoding shape should parse");

        let batch_enc = config
            .batch_encoding
            .expect("batch_encoding should be Some");
        let GcsBatchEncoding::Parquet(ref p) = batch_enc;
        assert_eq!(p.schema_mode, ParquetSchemaMode::AutoInfer);
        assert_eq!(p.compression, ParquetCompression::Snappy);
    }

    #[cfg(feature = "codecs-parquet")]
    fn parquet_sink_config() -> GcsSinkConfig {
        use vector_lib::codecs::encoding::format::{
            ParquetCompression, ParquetSchemaMode, ParquetSerializerConfig,
        };

        let parquet_config = ParquetSerializerConfig {
            schema_mode: ParquetSchemaMode::AutoInfer,
            compression: ParquetCompression::Snappy,
            ..Default::default()
        };

        GcsSinkConfig {
            batch_encoding: Some(GcsBatchEncoding::Parquet(parquet_config)),
            compression: Compression::None,
            ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
        }
    }

    /// Content-Type must be auto-detected as `application/vnd.apache.parquet`
    /// when `batch_encoding` is set and `content_type` is not explicitly provided.
    #[cfg(feature = "codecs-parquet")]
    #[test]
    fn parquet_content_type_auto_detected() {
        let request_settings = request_settings(&parquet_sink_config(), SinkContext::default());
        assert_eq!(
            request_settings.content_type.to_str().unwrap(),
            "application/vnd.apache.parquet",
            "Content-Type must be auto-detected for Parquet"
        );
    }

    /// When user explicitly sets `content_type`, the auto-detection must not override it.
    #[cfg(feature = "codecs-parquet")]
    #[test]
    fn parquet_content_type_user_override_preserved() {
        let sink_config = GcsSinkConfig {
            content_type: Some("application/octet-stream".to_string()),
            ..parquet_sink_config()
        };
        let request_settings = request_settings(&sink_config, SinkContext::default());
        assert_eq!(
            request_settings.content_type.to_str().unwrap(),
            "application/octet-stream",
            "User-specified Content-Type must not be overridden"
        );
    }

    /// The filename extension defaults to `parquet` when `batch_encoding` is set.
    #[cfg(feature = "codecs-parquet")]
    #[test]
    fn parquet_filename_extension_default() {
        let request_settings = request_settings(&parquet_sink_config(), SinkContext::default());
        assert_eq!(request_settings.extension, "parquet");
    }

    /// An explicit `filename_extension` overrides the `.parquet` default.
    #[cfg(feature = "codecs-parquet")]
    #[test]
    fn parquet_filename_extension_user_override() {
        let sink_config = GcsSinkConfig {
            filename_extension: Some("pq".to_string()),
            ..parquet_sink_config()
        };
        let request_settings = request_settings(&sink_config, SinkContext::default());
        assert_eq!(request_settings.extension, "pq");
    }

    /// Top-level compression is bypassed when `batch_encoding` is set, since
    /// Parquet handles compression internally.
    #[cfg(feature = "codecs-parquet")]
    #[test]
    fn parquet_top_level_compression_bypassed() {
        let sink_config = GcsSinkConfig {
            // Even with gzip requested, Parquet encoding bypasses it.
            compression: Compression::gzip_default(),
            ..parquet_sink_config()
        };
        let request_settings = request_settings(&sink_config, SinkContext::default());
        assert_eq!(request_settings.compression, Compression::None);
        // No outer Content-Encoding since compression is bypassed.
        assert!(request_settings.content_encoding.is_none());
    }

    /// Codecs other than `parquet` must be rejected at parse time, since
    /// `GcsBatchEncoding` only exposes the `parquet` variant.
    #[cfg(feature = "codecs-parquet")]
    #[test]
    fn parquet_batch_encoding_rejects_unsupported_codec() {
        let err = serde_yaml::from_str::<GcsSinkConfig>(
            r#"
            bucket: test-bucket
            compression: none
            encoding:
              codec: text
            batch_encoding:
              codec: arrow_stream
            "#,
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("arrow_stream"),
            "expected error to mention the offending codec, got: {err}"
        );
    }

    /// `schema_mode` defaults to `relaxed` when not specified.
    #[cfg(feature = "codecs-parquet")]
    #[test]
    fn parquet_schema_mode_defaults_to_relaxed() {
        use vector_lib::codecs::encoding::format::ParquetSchemaMode;

        let config: GcsSinkConfig = toml::from_str(
            r#"
            bucket = "test-bucket"
            compression = "none"

            [encoding]
            codec = "text"

            [batch_encoding]
            codec = "parquet"
            "#,
        )
        .unwrap();

        let GcsBatchEncoding::Parquet(p) = config.batch_encoding.unwrap();
        assert_eq!(p.schema_mode, ParquetSchemaMode::Relaxed);
    }

    /// End-to-end encoding: a batch of events is encoded into a valid Parquet
    /// file through the sink's request builder. Validates the Parquet magic
    /// bytes, the row count, and the inferred columns — exercising the real
    /// encoding path without requiring a live GCS backend.
    #[cfg(feature = "codecs-parquet")]
    #[test]
    fn parquet_encodes_valid_file() {
        use bytes::Bytes;
        use parquet::file::reader::{FileReader, SerializedFileReader};
        use parquet::record::reader::RowIter;

        let request_settings = request_settings(&parquet_sink_config(), SinkContext::default());

        let events: Vec<Event> = (0..10)
            .map(|i| {
                let mut log = LogEvent::from(format!("message_{i}"));
                log.insert("host", format!("host_{}", i % 3));
                Event::from(log)
            })
            .collect();

        let payload = request_settings
            .encode_events(events)
            .expect("parquet encoding should succeed");
        let body = payload.into_payload();

        assert!(body.len() >= 4, "Output too short to be valid Parquet");
        assert_eq!(&body[..4], b"PAR1", "Missing Parquet magic bytes");

        let reader =
            SerializedFileReader::new(Bytes::copy_from_slice(&body)).expect("Invalid Parquet file");
        let row_count = RowIter::from_file_into(Box::new(reader)).count();
        assert_eq!(row_count, 10, "Expected 10 rows in Parquet file");

        let reader =
            SerializedFileReader::new(Bytes::copy_from_slice(&body)).expect("Invalid Parquet file");
        let columns: Vec<String> = reader
            .metadata()
            .file_metadata()
            .schema_descr()
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        assert!(
            columns.contains(&"message".to_string()),
            "expected a `message` column, got: {columns:?}"
        );
        assert!(
            columns.contains(&"host".to_string()),
            "expected a `host` column, got: {columns:?}"
        );
    }
}
