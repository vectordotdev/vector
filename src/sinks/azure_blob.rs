use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext},
    sinks::{
        util::{
            encoding::{EncodingConfig, EncodingConfiguration},
            retries::RetryLogic,
            sink::Response,
            BatchConfig, BatchSettings, Buffer, Compression, Concurrency, PartitionBatchSink,
            PartitionBuffer, PartitionInnerBuffer, ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    template::Template,
    Event, Result,
};
use azure_sdk_core::{
    errors::AzureError, BlobNameSupport, BodySupport, ContainerNameSupport, ContentEncodingSupport,
    ContentTypeSupport,
};
use azure_sdk_storage_blob::{blob::responses::PutBlockBlobResponse, Blob, Container};
use azure_sdk_storage_core::{key_client::KeyClient, prelude::client::from_connection_string};
use bytes::Bytes;
use chrono::Utc;
use futures::{future::BoxFuture, stream, FutureExt, SinkExt, StreamExt};
use http::StatusCode;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    convert::TryFrom,
    result::Result as StdResult,
    task::{Context, Poll},
};
use tower::{Service, ServiceBuilder};
use tracing_futures::Instrument;
use uuid::Uuid;

#[derive(Clone)]
pub struct AzureBlobSink {
    client: KeyClient,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct AzureBlobSinkConfig {
    pub connection_string: String,
    pub container_name: String,
    pub blob_prefix: Option<String>,
    pub blob_time_format: Option<String>,
    pub blob_append_uuid: Option<bool>,
    pub encoding: EncodingConfig<Encoding>,
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
}

#[derive(Debug, Clone)]
struct AzureBlobRetryLogic;

#[derive(Debug, Clone)]
struct AzureBlobSinkRequest {
    container_name: String,
    blob_name: String,
    blob_data: Vec<u8>,
    content_encoding: Option<&'static str>,
    content_type: Option<&'static str>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Json,
    Text,
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Invalid connection string specified"))]
    InvalidCredentials,
    #[snafu(display("Container: {:?} not found", container))]
    UnknownContainer { container: String },
    #[snafu(display("Unknown status code: {}", status))]
    Unknown { status: StatusCode },
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        concurrency: Concurrency::Fixed(50),
        rate_limit_num: Some(250),
        ..Default::default()
    };
}

impl GenerateConfig for AzureBlobSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            connection_string: String::from("DefaultEndpointsProtocol=https;AccountName=some-account-name;AccountKey=some-account-key;EndpointSuffix=core.windows.net"),
            container_name: String::from("logs"),
            blob_prefix: Some(String::from("blob")),
            blob_time_format: Some(String::from("%s")),
            blob_append_uuid: Some(true),
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
        let client = self.create_client()?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();
        let sink = self.new(client, cx)?;
        Ok((sink, healthcheck))
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
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);
        let batch = BatchSettings::default()
            .bytes(10 * 1024 * 1024)
            .timeout(300)
            .parse_config(self.batch)?;
        let compression = self.compression;
        let container_name = self.container_name.clone();
        let blob_time_format = self.blob_time_format.clone().unwrap_or_else(|| "%s".into());
        let blob_append_uuid = self.blob_append_uuid.unwrap_or(true);
        let blob = AzureBlobSink { client };
        let svc = ServiceBuilder::new()
            .map(move |partition| {
                build_request(
                    partition,
                    compression,
                    container_name.clone(),
                    blob_time_format.clone(),
                    blob_append_uuid,
                )
            })
            .settings(request, AzureBlobRetryLogic)
            .service(blob);

        let encoding = self.encoding.clone();
        let blob_prefix = self.blob_prefix.as_deref().unwrap_or_else(|| "blob".into());
        let blob_prefix = Template::try_from(blob_prefix)?;
        let buffer = PartitionBuffer::new(Buffer::new(batch.size, compression));
        let sink = PartitionBatchSink::new(svc, buffer, batch.timeout, cx.acker())
            .with_flat_map(move |event| {
                stream::iter(encode_event(event, &blob_prefix, &encoding)).map(Ok)
            })
            .sink_map_err(|error| error!(message = "Sink failed to flush.", %error));

        Ok(super::VectorSink::Sink(Box::new(sink)))
    }

    pub async fn healthcheck(self, client: KeyClient) -> Result<()> {
        let container_name = self.container_name.clone();
        let request = client
            .get_container_properties()
            .with_container_name(container_name.as_str())
            .finalize();

        match request.await {
            Ok(_) => Ok(()),
            Err(reason) => Err(match reason {
                AzureError::UnexpectedHTTPResult(result) => match result.status_code() {
                    StatusCode::FORBIDDEN => HealthcheckError::InvalidCredentials.into(),
                    StatusCode::NOT_FOUND => HealthcheckError::UnknownContainer {
                        container: container_name,
                    }
                    .into(),
                    status => HealthcheckError::Unknown { status }.into(),
                },
                error => error.into(),
            }),
        }
    }

    pub fn create_client(&self) -> Result<KeyClient> {
        let connection_string = self.connection_string.clone();
        let client = from_connection_string(connection_string.as_str())?;

        Ok(client)
    }
}

impl Service<AzureBlobSinkRequest> for AzureBlobSink {
    type Response = PutBlockBlobResponse;
    type Error = AzureError;
    type Future = BoxFuture<'static, StdResult<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<StdResult<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: AzureBlobSinkRequest) -> Self::Future {
        let client = self.client.clone();
        let container_name = request.container_name.clone();
        let blob_name = request.blob_name.clone();
        let blob_data = request.blob_data.clone();

        Box::pin(async move {
            client
                .put_block_blob()
                .with_container_name(container_name.as_str())
                .with_blob_name(blob_name.as_str())
                .with_body(blob_data.as_slice())
                .with_content_encoding(request.content_encoding.unwrap())
                .with_content_type(request.content_type.unwrap())
                .finalize()
                .instrument(info_span!("request"))
                .await
        })
    }
}

impl Compression {
    pub fn content_type(&self) -> Option<&'static str> {
        match self {
            Self::None => Some("text/plain"),
            Self::Gzip(_) => Some("application/octet-stream"),
        }
    }
}

impl Response for PutBlockBlobResponse {}

impl RetryLogic for AzureBlobRetryLogic {
    type Error = AzureError;
    type Response = PutBlockBlobResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            AzureError::UnexpectedHTTPResult(result) => {
                let status_code = result.status_code();
                status_code.is_server_error() || status_code == StatusCode::TOO_MANY_REQUESTS
            }
            _ => false,
        }
    }
}

fn encode_event(
    mut event: Event,
    blob_prefix: &Template,
    encoding: &EncodingConfig<Encoding>,
) -> Option<PartitionInnerBuffer<Vec<u8>, Bytes>> {
    let key = blob_prefix
        .render_string(&event)
        .map_err(|missing_keys| {
            warn!(
                message = "Keys do not exist on the event; dropping event.",
                ?missing_keys,
                internal_log_rate_secs = 30,
            );
        })
        .ok()?;

    encoding.apply_rules(&mut event);

    let log = event.into_log();
    let bytes = match encoding.codec() {
        Encoding::Json => {
            serde_json::to_vec(&log).expect("Failed to encode event as json, this is a bug!")
        }
        Encoding::Text => {
            let mut bytes = log
                .get(log_schema().message_key())
                .map(|v| v.as_bytes().to_vec())
                .expect("Failed to encode event as text");
            bytes.push(b'\n');
            bytes
        }
    };

    Some(PartitionInnerBuffer::new(bytes, key.into()))
}

fn build_request(
    partition: PartitionInnerBuffer<Vec<u8>, Bytes>,
    compression: Compression,
    container_name: String,
    blob_time_format: String,
    blob_append_uuid: bool,
) -> AzureBlobSinkRequest {
    let (inner, key) = partition.into_parts();
    let filename = {
        let time_format = Utc::now().format(&blob_time_format);

        if blob_append_uuid {
            let uuid = Uuid::new_v4();
            format!("{}-{}", time_format.to_string(), uuid.to_hyphenated())
        } else {
            time_format.to_string()
        }
    };
    let blob = String::from_utf8_lossy(&key[..]).into_owned();
    let blob = format!("{}{}.{}", blob, filename, compression.extension());

    debug!(
        message = "Sending events.",
        bytes = ?inner.len(),
        container = ?container_name,
        blob = ?blob
    );

    AzureBlobSinkRequest {
        container_name,
        blob_data: inner,
        blob_name: blob,
        content_encoding: compression.content_encoding(),
        content_type: compression.content_type(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AzureBlobSinkConfig>();
    }

    #[test]
    fn azure_blob_encode_event_text() {
        let message = String::from("hello world");
        let event = Event::from(message.clone());
        let blob_prefix = Template::try_from("logs/blob/%F").unwrap();
        let encoding = EncodingConfig {
            codec: Encoding::Text,
            schema: None,
            only_fields: None,
            except_fields: None,
            timestamp_format: None,
        };

        let bytes = encode_event(event, &blob_prefix, &encoding).unwrap();

        let encoded_message = message + "\n";
        let (bytes, _) = bytes.into_parts();
        assert_eq!(&bytes[..], encoded_message.as_bytes());
    }

    #[test]
    fn azure_blob_encode_event_json() {
        let message = String::from("hello world");
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");
        let blob_prefix = Template::try_from("logs/blob/%F").unwrap();
        let encoding = EncodingConfig {
            codec: Encoding::Json,
            schema: None,
            only_fields: None,
            except_fields: None,
            timestamp_format: None,
        };

        let bytes = encode_event(event, &blob_prefix, &encoding).unwrap();

        let (bytes, _) = bytes.into_parts();
        let map: BTreeMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();
        assert_eq!(map[&log_schema().message_key().to_string()], message);
        assert_eq!(map["key"], "value".to_string());
    }

    #[test]
    fn azure_blob_encode_event_with_removed_key() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");
        let blob_prefix = Template::try_from("logs/blob/%F").unwrap();
        let encoding = EncodingConfig {
            codec: Encoding::Json,
            schema: None,
            only_fields: None,
            except_fields: Some(vec!["key".into()]),
            timestamp_format: None,
        };

        let bytes = encode_event(event, &blob_prefix, &encoding).unwrap();

        let (bytes, _) = bytes.into_parts();
        let map: BTreeMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();
        assert_eq!(map[&log_schema().message_key().to_string()], message);
    }

    #[test]
    fn azure_blob_build_request_without_compression() {
        let partition = PartitionInnerBuffer::new(vec![0u8; 10], Bytes::from("blob"));
        let compression = Compression::None;
        let container_name = String::from("logs");
        let blob_time_format = String::from("");
        let blob_append_uuid = false;

        let request = build_request(
            partition,
            compression,
            container_name,
            blob_time_format,
            blob_append_uuid,
        );

        assert_eq!(request.container_name, "logs".to_string());
        assert_eq!(request.blob_name, "blob.log".to_string());
        assert_eq!(request.content_encoding, None);
        assert_eq!(request.content_type.unwrap(), "text/plain");
    }

    #[test]
    fn azure_blob_build_request_with_compression() {
        let partition = PartitionInnerBuffer::new(vec![0u8; 10], Bytes::from("blob"));
        let compression = Compression::gzip_default();
        let container_name = String::from("logs");
        let blob_time_format = String::from("");
        let blob_append_uuid = false;

        let request = build_request(
            partition,
            compression,
            container_name,
            blob_time_format,
            blob_append_uuid,
        );

        assert_eq!(request.container_name, "logs".to_string());
        assert_eq!(request.blob_name, "blob.log.gz".to_string());
        assert_eq!(request.content_encoding, Some("gzip"));
        assert_eq!(request.content_type.unwrap(), "application/octet-stream");
    }

    #[test]
    fn azure_blob_build_request_with_time_format() {
        let partition = PartitionInnerBuffer::new(vec![0u8; 10], Bytes::from("blob"));
        let compression = Compression::None;
        let container_name = String::from("logs");
        let blob_time_format = String::from("%F");
        let blob_append_uuid = false;

        let request = build_request(
            partition,
            compression,
            container_name,
            blob_time_format,
            blob_append_uuid,
        );

        assert_eq!(request.container_name, "logs".to_string());
        assert_eq!(request.blob_name, format!("blob{}.log", Utc::now().format("%F")));
        assert_eq!(request.content_encoding, None);
        assert_eq!(request.content_type.unwrap(), "text/plain");
    }

    #[test]
    fn azure_blob_build_request_with_uuid() {
        let partition = PartitionInnerBuffer::new(vec![0u8; 10], Bytes::from("blob"));
        let compression = Compression::None;
        let container_name = String::from("logs");
        let blob_time_format = String::from("");
        let blob_append_uuid = true;

        let request = build_request(
            partition,
            compression,
            container_name,
            blob_time_format,
            blob_append_uuid,
        );

        assert_eq!(request.container_name, "logs".to_string());
        assert_ne!(request.blob_name, "blob.log".to_string());
        assert_eq!(request.content_encoding, None);
        assert_eq!(request.content_type.unwrap(), "text/plain");
    }
}
