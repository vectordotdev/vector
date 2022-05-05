use std::convert::TryInto;

use aws_sdk_s3::Client as S3Client;
use codecs::encoding::{Framer, Serializer};
use codecs::{CharacterDelimitedEncoder, NewlineDelimitedEncoder};
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use vector_core::sink::VectorSink;

use super::sink::S3RequestOptions;
use crate::aws::{AwsAuthentication, RegionOrEndpoint};
use crate::sinks::util::encoding::EncodingConfigWithFramingAdapter;
use crate::{
    codecs::Encoder,
    config::{AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext},
    sinks::{
        s3_common::{
            self,
            config::{S3Options, S3RetryLogic},
            service::S3Service,
            sink::S3Sink,
        },
        util::{
            encoding::{EncodingConfig, StandardEncodings, StandardEncodingsWithFramingMigrator},
            partitioner::KeyPartitioner,
            BatchConfig, BulkSizeBasedDefaultBatchSettings, Compression, ServiceBuilderExt,
            TowerRequestConfig,
        },
        Healthcheck,
    },
    tls::TlsConfig,
};

const DEFAULT_KEY_PREFIX: &str = "date=%F/";
const DEFAULT_FILENAME_TIME_FORMAT: &str = "%s";
const DEFAULT_FILENAME_APPEND_UUID: bool = true;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct S3SinkConfig {
    pub bucket: String,
    pub key_prefix: Option<String>,
    pub filename_time_format: Option<String>,
    pub filename_append_uuid: Option<bool>,
    pub filename_extension: Option<String>,
    #[serde(flatten)]
    pub options: S3Options,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    #[serde(flatten)]
    pub encoding: EncodingConfigWithFramingAdapter<
        EncodingConfig<StandardEncodings>,
        StandardEncodingsWithFramingMigrator,
    >,
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig<BulkSizeBasedDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsConfig>,
    #[serde(default)]
    pub auth: AwsAuthentication,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for S3SinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            bucket: "".to_owned(),
            key_prefix: None,
            filename_time_format: None,
            filename_append_uuid: None,
            filename_extension: None,
            options: S3Options::default(),
            region: RegionOrEndpoint::default(),
            encoding: EncodingConfig::from(StandardEncodings::Text).into(),
            compression: Compression::gzip_default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls: Some(TlsConfig::default()),
            auth: AwsAuthentication::default(),
            acknowledgements: Default::default(),
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
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "aws_s3"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
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
        let request_limits = self.request.unwrap_with(&Default::default());
        let service = ServiceBuilder::new()
            .settings(request_limits, S3RetryLogic)
            .service(service);

        // Configure our partitioning/batching.
        let batch_settings = self.batch.into_batcher_settings()?;
        let key_prefix = self
            .key_prefix
            .as_ref()
            .cloned()
            .unwrap_or_else(|| DEFAULT_KEY_PREFIX.into())
            .try_into()?;
        let partitioner = KeyPartitioner::new(key_prefix);

        // And now collect all of the S3-specific options and configuration knobs.
        let filename_time_format = self
            .filename_time_format
            .as_ref()
            .cloned()
            .unwrap_or_else(|| DEFAULT_FILENAME_TIME_FORMAT.into());
        let filename_append_uuid = self
            .filename_append_uuid
            .unwrap_or(DEFAULT_FILENAME_APPEND_UUID);

        let transformer = self.encoding.transformer();
        let (framer, serializer) = self.encoding.clone().encoding();
        let framer = match (framer, &serializer) {
            (Some(framer), _) => framer,
            (None, Serializer::Json(_)) => CharacterDelimitedEncoder::new(b',').into(),
            (None, Serializer::Native(_)) => {
                // TODO: We probably want to use something like octet framing here.
                return Err("Native encoding is not implemented for this sink yet".into());
            }
            (None, Serializer::NativeJson(_) | Serializer::RawMessage(_)) => {
                NewlineDelimitedEncoder::new().into()
            }
        };
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let request_options = S3RequestOptions {
            bucket: self.bucket.clone(),
            api_options: self.options.clone(),
            filename_extension: self.filename_extension.clone(),
            filename_time_format,
            filename_append_uuid,
            encoder: (transformer, encoder),
            compression: self.compression,
        };

        let sink = S3Sink::new(cx, service, request_options, partitioner, batch_settings);

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
