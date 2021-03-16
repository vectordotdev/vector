use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext},
    sinks::{
        util::{
            encoding::EncodingConfig, sink::Response, BatchConfig, Compression, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    Result,
};
use azure_sdk_core::errors::AzureError;
use azure_sdk_storage_blob::blob::responses::PutBlockBlobResponse;
use azure_sdk_storage_core::key_client::KeyClient;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::{
    result::Result as StdResult,
    task::{Context, Poll},
};
use tower::Service;

#[derive(Clone)]
pub struct AzureBlobSink {
    client: KeyClient,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct AzureBlobSinkConfig {
    pub connection_string: Option<String>,
    pub container_name: Option<String>,
    pub blob_prefix: Option<String>,
    pub blob_time_format: Option<String>,
    pub encoding: EncodingConfig<Encoding>,
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
}

#[derive(Debug, Clone)]
struct AzureBlobSinkRequest {
    container_name: String,
    blob_name: String,
    blob_data: Vec<u8>,
    content_encoding: Option<&'static str>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Json,
}

impl GenerateConfig for AzureBlobSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            connection_string: None,
            container_name: Option::Some(String::from("logs")),
            blob_prefix: None,
            blob_time_format: None,
            encoding: Encoding::Json.into(),
            compression: Compression::gzip_default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "azure_blob")]
impl SinkConfig for AzureBlobSinkConfig {
    async fn build(&self, cx: SinkContext) -> Result<(VectorSink, Healthcheck)> {
        unimplemented!()
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "azure_blob"
    }
}

impl AzureBlobSinkConfig {
    pub fn new(&self, client: KeyClient, cx: SinkContext) -> Result<VectorSink> {
        unimplemented!()
    }

    pub async fn healthcheck(&self, client: KeyClient) -> Result<()> {
        unimplemented!()
    }

    pub fn create_client(&self) -> Result<KeyClient> {
        unimplemented!()
    }
}

impl Response for PutBlockBlobResponse {}

impl Service<AzureBlobSinkRequest> for AzureBlobSink {
    type Response = PutBlockBlobResponse;
    type Error = AzureError;
    type Future = BoxFuture<'static, StdResult<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<StdResult<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: AzureBlobSinkRequest) -> Self::Future {
        unimplemented!()
    }
}
