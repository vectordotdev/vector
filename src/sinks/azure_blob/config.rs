use std::{convert::TryInto, sync::Arc};

use azure_storage_blobs::prelude::*;
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;

use super::service::AzureBlobRequestOptions;

use crate::{
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{
        azure_common::{
            self, config::AzureBlobRetryLogic, service::AzureBlobService, sink::AzureBlobSink,
        },
        util::{
            encoding::{EncodingConfig, StandardEncodings},
            partitioner::KeyPartitioner,
            BatchConfig, BulkSizeBasedDefaultBatchSettings, Compression, ServiceBuilderExt,
            TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    Result,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct AzureBlobSinkConfig {
    pub connection_string: String,
    pub(super) container_name: String,
    pub blob_prefix: Option<String>,
    pub blob_time_format: Option<String>,
    pub blob_append_uuid: Option<bool>,
    pub encoding: EncodingConfig<StandardEncodings>,
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig<BulkSizeBasedDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub(super) acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for AzureBlobSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            connection_string: String::from("DefaultEndpointsProtocol=https;AccountName=some-account-name;AccountKey=some-account-key;"),
            container_name: String::from("logs"),
            blob_prefix: Some(String::from("blob")),
            blob_time_format: Some(String::from("%s")),
            blob_append_uuid: Some(true),
            encoding: StandardEncodings::Ndjson.into(),
            compression: Compression::gzip_default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "azure_blob")]
impl SinkConfig for AzureBlobSinkConfig {
    async fn build(&self, cx: SinkContext) -> Result<(VectorSink, Healthcheck)> {
        let client = azure_common::config::build_client(
            self.connection_string.clone(),
            self.container_name.clone(),
        )?;
        let healthcheck = azure_common::config::build_healthcheck(
            self.container_name.clone(),
            Arc::clone(&client),
        )?;
        let sink = self.build_processor(client, cx)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "azure_blob"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
    }
}

const DEFAULT_REQUEST_LIMITS: TowerRequestConfig =
    TowerRequestConfig::const_default().rate_limit_num(250);

const DEFAULT_KEY_PREFIX: &str = "blob/%F/";
const DEFAULT_FILENAME_TIME_FORMAT: &str = "%s";
const DEFAULT_FILENAME_APPEND_UUID: bool = true;

impl AzureBlobSinkConfig {
    pub fn build_processor(
        &self,
        client: Arc<ContainerClient>,
        cx: SinkContext,
    ) -> crate::Result<VectorSink> {
        let request_limits = self.request.unwrap_with(&DEFAULT_REQUEST_LIMITS);
        let service = ServiceBuilder::new()
            .settings(request_limits, AzureBlobRetryLogic)
            .service(AzureBlobService::new(client));

        // Configure our partitioning/batching.
        let batcher_settings = self.batch.into_batcher_settings()?;

        let blob_time_format = self
            .blob_time_format
            .as_ref()
            .cloned()
            .unwrap_or_else(|| DEFAULT_FILENAME_TIME_FORMAT.into());
        let blob_append_uuid = self
            .blob_append_uuid
            .unwrap_or(DEFAULT_FILENAME_APPEND_UUID);

        let request_options = AzureBlobRequestOptions {
            container_name: self.container_name.clone(),
            blob_time_format,
            blob_append_uuid,
            encoding: self.encoding.clone(),
            compression: self.compression,
        };

        let sink = AzureBlobSink::new(
            cx,
            service,
            request_options,
            self.key_partitioner()?,
            batcher_settings,
        );

        Ok(VectorSink::from_event_streamsink(sink))
    }

    pub fn key_partitioner(&self) -> crate::Result<KeyPartitioner> {
        let blob_prefix = self
            .blob_prefix
            .as_ref()
            .cloned()
            .unwrap_or_else(|| DEFAULT_KEY_PREFIX.into())
            .try_into()?;
        Ok(KeyPartitioner::new(blob_prefix))
    }
}
