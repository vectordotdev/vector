use std::convert::TryInto;

use rusoto_s3::S3Client;
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use vector_core::sink::VectorSink;

use super::sink::S3RequestOptions;
use crate::{
    aws::rusoto::{AwsAuthentication, RegionOrEndpoint},
    config::{DataType, GenerateConfig, ProxyConfig, SinkConfig, SinkContext},
    sinks::{
        s3_common::{
            self,
            config::{S3Options, S3RetryLogic},
            service::S3Service,
            sink::S3Sink,
        },
        util::{
            encoding::{EncodingConfig, StandardEncodings},
            partitioner::KeyPartitioner,
            BatchConfig, BulkSizeBasedDefaultBatchSettings, Compression, ServiceBuilderExt,
            TowerRequestConfig,
        },
        Healthcheck,
    },
};

const DEFAULT_KEY_PREFIX: &str = "date=%F/";
const DEFAULT_FILENAME_TIME_FORMAT: &str = "%s";
const DEFAULT_FILENAME_APPEND_UUID: bool = true;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
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
    pub encoding: EncodingConfig<StandardEncodings>,
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig<BulkSizeBasedDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    // Deprecated name. Moved to auth.
    pub assume_role: Option<String>,
    #[serde(default)]
    pub auth: AwsAuthentication,
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
            encoding: StandardEncodings::Text.into(),
            compression: Compression::gzip_default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            assume_role: None,
            auth: AwsAuthentication::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_s3")]
impl SinkConfig for S3SinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let service = self.create_service(&cx.proxy)?;
        let healthcheck = self.build_healthcheck(service.client())?;
        let sink = self.build_processor(service, cx)?;
        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_s3"
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

        let request_options = S3RequestOptions {
            bucket: self.bucket.clone(),
            api_options: self.options.clone(),
            filename_extension: self.filename_extension.clone(),
            filename_time_format,
            filename_append_uuid,
            encoding: self.encoding.clone(),
            compression: self.compression,
        };

        let sink = S3Sink::new(cx, service, request_options, partitioner, batch_settings);

        Ok(VectorSink::from_event_streamsink(sink))
    }

    pub fn build_healthcheck(&self, client: S3Client) -> crate::Result<Healthcheck> {
        s3_common::config::build_healthcheck(self.bucket.clone(), client)
    }

    pub fn create_service(&self, proxy: &ProxyConfig) -> crate::Result<S3Service> {
        s3_common::config::create_service(&self.region, &self.auth, self.assume_role.clone(), proxy)
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
