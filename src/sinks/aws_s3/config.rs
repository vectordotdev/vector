use aws_sdk_s3::Client as S3Client;
use tower::ServiceBuilder;
use vector_lib::codecs::{
    encoding::{Framer, FramingConfig},
    TextSerializerConfig,
};
use vector_lib::configurable::configurable_component;
use vector_lib::sink::VectorSink;
use vector_lib::TimeZone;

use super::sink::S3RequestOptions;
use crate::{
    aws::{AwsAuthentication, RegionOrEndpoint},
    codecs::{Encoder, EncodingConfigWithFraming, SinkType},
    config::{AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext},
    sinks::{
        s3_common::{
            self,
            config::{S3Options, S3RetryLogic},
            partitioner::S3KeyPartitioner,
            service::S3Service,
            sink::S3Sink,
        },
        util::{
            timezone_to_offset, BatchConfig, BulkSizeBasedDefaultBatchSettings, Compression,
            ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck,
    },
    template::Template,
    tls::TlsConfig,
};

/// Configuration for the `aws_s3` sink.
#[configurable_component(sink(
    "aws_s3",
    "Store observability events in the AWS S3 object storage system."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct S3SinkConfig {
    /// The S3 bucket name.
    ///
    /// This must not include a leading `s3://` or a trailing `/`.
    #[configurable(metadata(docs::examples = "my-bucket"))]
    pub bucket: String,

    /// A prefix to apply to all object keys.
    ///
    /// Prefixes are useful for partitioning objects, such as by creating an object key that
    /// stores objects under a particular directory. If using a prefix for this purpose, it must end
    /// in `/` to act as a directory path. A trailing `/` is **not** automatically added.
    #[serde(default = "default_key_prefix")]
    #[configurable(metadata(docs::templateable))]
    #[configurable(metadata(docs::examples = "date=%F/hour=%H"))]
    #[configurable(metadata(docs::examples = "year=%Y/month=%m/day=%d"))]
    #[configurable(metadata(docs::examples = "application_id={{ application_id }}/date=%F"))]
    pub key_prefix: String,

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
    #[serde(default = "default_filename_time_format")]
    pub filename_time_format: String,

    /// Whether or not to append a UUID v4 token to the end of the object key.
    ///
    /// The UUID is appended to the timestamp portion of the object key, such that if the object key
    /// generated is `date=2022-07-18/1658176486`, setting this field to `true` results
    /// in an object key that looks like `date=2022-07-18/1658176486-30f6652c-71da-4f9f-800d-a1189c47c547`.
    ///
    /// This ensures there are no name collisions, and can be useful in high-volume workloads where
    /// object keys must be unique.
    #[serde(default = "crate::serde::default_true")]
    #[configurable(metadata(docs::human_name = "Append UUID to Filename"))]
    pub filename_append_uuid: bool,

    /// The filename extension to use in the object key.
    ///
    /// This overrides setting the extension based on the configured `compression`.
    #[configurable(metadata(docs::examples = "json"))]
    pub filename_extension: Option<String>,

    #[serde(flatten)]
    pub options: S3Options,

    #[serde(flatten)]
    pub region: RegionOrEndpoint,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,

    /// Compression configuration.
    ///
    /// All compression algorithms use the default compression level unless otherwise specified.
    ///
    /// Some cloud storage API clients and browsers handle decompression transparently, so
    /// depending on how they are accessed, files may not always appear to be compressed.
    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<BulkSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub auth: AwsAuthentication,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub timezone: Option<TimeZone>,
}

pub(super) fn default_key_prefix() -> String {
    "date=%F".to_string()
}

pub(super) fn default_filename_time_format() -> String {
    "%s".to_string()
}

impl GenerateConfig for S3SinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            bucket: "".to_owned(),
            key_prefix: default_key_prefix(),
            filename_time_format: default_filename_time_format(),
            filename_append_uuid: true,
            filename_extension: None,
            options: S3Options::default(),
            region: RegionOrEndpoint::default(),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Compression::gzip_default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls: Some(TlsConfig::default()),
            auth: AwsAuthentication::default(),
            acknowledgements: Default::default(),
            timezone: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_s3")]
impl SinkConfig for S3SinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let service = self.create_service(&cx.proxy).await?;
        let healthcheck = self.build_healthcheck(service.client())?;
        let sink = self.build_processor(service, cx)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl S3SinkConfig {
    pub fn build_processor(
        &self,
        service: S3Service,
        cx: SinkContext,
    ) -> crate::Result<VectorSink> {
        // Build our S3 client/service, which is what we'll ultimately feed
        // requests into in order to ship files to S3.  We build this here in
        // order to configure the client/service with retries, concurrency
        // limits, rate limits, and whatever else the client should have.
        let request_limits = self.request.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_limits, S3RetryLogic)
            .service(service);

        let offset = self
            .timezone
            .or(cx.globals.timezone)
            .and_then(timezone_to_offset);

        // Configure our partitioning/batching.
        let batch_settings = self.batch.into_batcher_settings()?;

        let key_prefix = Template::try_from(self.key_prefix.clone())?.with_tz_offset(offset);

        let ssekms_key_id = self
            .options
            .ssekms_key_id
            .as_ref()
            .cloned()
            .map(|ssekms_key_id| Template::try_from(ssekms_key_id.as_str()))
            .transpose()?;

        let partitioner = S3KeyPartitioner::new(key_prefix, ssekms_key_id);

        let transformer = self.encoding.transformer();
        let (framer, serializer) = self.encoding.build(SinkType::MessageBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let request_options = S3RequestOptions {
            bucket: self.bucket.clone(),
            api_options: self.options.clone(),
            filename_extension: self.filename_extension.clone(),
            filename_time_format: self.filename_time_format.clone(),
            filename_append_uuid: self.filename_append_uuid,
            encoder: (transformer, encoder),
            compression: self.compression,
            filename_tz_offset: offset,
        };

        let sink = S3Sink::new(service, request_options, partitioner, batch_settings);

        Ok(VectorSink::from_event_streamsink(sink))
    }

    pub fn build_healthcheck(&self, client: S3Client) -> crate::Result<Healthcheck> {
        s3_common::config::build_healthcheck(self.bucket.clone(), client)
    }

    pub async fn create_service(&self, proxy: &ProxyConfig) -> crate::Result<S3Service> {
        s3_common::config::create_service(&self.region, &self.auth, proxy, &self.tls).await
    }
}

#[cfg(test)]
mod tests {
    use super::S3SinkConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<S3SinkConfig>();
    }
}
