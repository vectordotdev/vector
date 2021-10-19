use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext},
    event::Event,
    internal_events::{
        azure_blob::{AzureBlobErrorResponse, AzureBlobEventSent, AzureBlobHttpError},
        TemplateRenderingFailed,
    },
    sinks::{
        util::{
            encoding::{EncodingConfig, EncodingConfiguration},
            retries::RetryLogic,
            sink::Response,
            BatchConfig, BatchSettings, Buffer, Compression, Concurrency, EncodedEvent,
            PartitionBatchSink, PartitionBuffer, PartitionInnerBuffer, ServiceBuilderExt,
            TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    template::Template,
    Result,
};
use azure_core::prelude::*;
use azure_core::HttpError;
use azure_storage::blob::blob::responses::PutBlockBlobResponse;
use azure_storage::blob::prelude::*;
use azure_storage::core::prelude::*;
use bytes::Bytes;
use chrono::Utc;
use futures::{future::BoxFuture, stream, FutureExt, SinkExt, StreamExt, TryFutureExt};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    convert::TryFrom,
    result::Result as StdResult,
    sync::Arc,
    task::{Context, Poll},
};
use tower::{Service, ServiceBuilder};
use tracing_futures::Instrument;
use uuid::Uuid;
use vector_core::ByteSizeOf;

#[derive(Clone)]
pub struct AzureBlobSink {
    client: Arc<ContainerClient>,
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
    content_type: &'static str,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Ndjson,
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

impl GenerateConfig for AzureBlobSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            connection_string: String::from("DefaultEndpointsProtocol=https;AccountName=some-account-name;AccountKey=some-account-key;"),
            container_name: String::from("logs"),
            blob_prefix: Some(String::from("blob")),
            blob_time_format: Some(String::from("%s")),
            blob_append_uuid: Some(true),
            encoding: Encoding::Ndjson.into(),
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
        let healthcheck = self.clone().healthcheck(Arc::clone(&client)).boxed();
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
    pub fn new(&self, client: Arc<ContainerClient>, cx: SinkContext) -> Result<VectorSink> {
        let request = self.request.unwrap_with(&TowerRequestConfig {
            concurrency: Concurrency::Fixed(50),
            rate_limit_num: Some(250),
            ..Default::default()
        });
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
        let blob_prefix = self.blob_prefix.as_deref().unwrap_or("blob/%F/");
        let blob_prefix = Template::try_from(blob_prefix)?;
        let buffer = PartitionBuffer::new(Buffer::new(batch.size, compression));
        let sink = PartitionBatchSink::new(svc, buffer, batch.timeout, cx.acker())
            .with_flat_map(move |event| {
                stream::iter(encode_event(event, &blob_prefix, &encoding)).map(Ok)
            })
            .sink_map_err(|error| error!(message = "Sink failed to flush.", %error));

        Ok(super::VectorSink::Sink(Box::new(sink)))
    }

    pub async fn healthcheck(self, client: Arc<ContainerClient>) -> Result<()> {
        let container_name = self.container_name.clone();
        let request = client.get_properties().execute().await;

        match request {
            Ok(_) => Ok(()),
            Err(reason) => Err(match reason.downcast_ref::<HttpError>() {
                Some(HttpError::UnexpectedStatusCode { received, .. }) => match *received {
                    StatusCode::FORBIDDEN => HealthcheckError::InvalidCredentials.into(),
                    StatusCode::NOT_FOUND => HealthcheckError::UnknownContainer {
                        container: container_name,
                    }
                    .into(),
                    status => HealthcheckError::Unknown { status }.into(),
                },
                _ => reason,
            }),
        }
    }

    pub fn create_client(&self) -> Result<Arc<ContainerClient>> {
        let connection_string = self.connection_string.as_str();
        let container_name = self.container_name.as_str();
        let client =
            StorageAccountClient::new_connection_string(new_http_client(), connection_string)?
                .as_storage_client()
                .as_container_client(container_name);

        Ok(client)
    }
}

impl Service<AzureBlobSinkRequest> for AzureBlobSink {
    type Response = PutBlockBlobResponse;
    type Error = Box<dyn std::error::Error + std::marker::Send + std::marker::Sync>;
    type Future = BoxFuture<'static, StdResult<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<StdResult<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: AzureBlobSinkRequest) -> Self::Future {
        let client = Arc::clone(&self.client).as_blob_client(request.blob_name.as_str());

        Box::pin(async move {
            let byte_size = request.blob_data.len();
            let blob = client
                .put_block_blob(Bytes::from(request.blob_data))
                .content_type(request.content_type);
            let blob = match request.content_encoding {
                Some(encoding) => blob.content_encoding(encoding),
                None => blob,
            };

            blob.execute()
                .inspect_err(|reason| {
                    match reason.downcast_ref::<HttpError>() {
                        Some(HttpError::UnexpectedStatusCode { received, .. }) => {
                            emit!(&AzureBlobErrorResponse { code: *received })
                        }
                        _ => emit!(&AzureBlobHttpError {
                            error: reason.to_string()
                        }),
                    };
                })
                .inspect_ok(|result| {
                    emit!(&AzureBlobEventSent {
                        request_id: result.request_id,
                        byte_size
                    })
                })
                .instrument(info_span!("request"))
                .await
        })
    }
}

impl Compression {
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::None => "text/plain",
            Self::Gzip(_) => "application/gzip",
        }
    }
}

impl Response for PutBlockBlobResponse {}

impl RetryLogic for AzureBlobRetryLogic {
    type Error = HttpError;
    type Response = PutBlockBlobResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            HttpError::UnexpectedStatusCode { received, .. } => {
                received.is_server_error() || received == &StatusCode::TOO_MANY_REQUESTS
            }
            _ => false,
        }
    }
}

fn encode_event(
    mut event: Event,
    blob_prefix: &Template,
    encoding: &EncodingConfig<Encoding>,
) -> Option<EncodedEvent<PartitionInnerBuffer<Vec<u8>, Bytes>>> {
    let key = blob_prefix
        .render_string(&event)
        .map_err(|error| {
            emit!(&TemplateRenderingFailed {
                error,
                field: Some("blob_prefix"),
                drop_event: true,
            });
        })
        .ok()?;

    let byte_size = event.size_of();
    encoding.apply_rules(&mut event);

    let log = event.into_log();
    let bytes = match encoding.codec() {
        Encoding::Ndjson => serde_json::to_vec(&log)
            .map(|mut b| {
                b.push(b'\n');
                b
            })
            .expect("Failed to encode event as json, this is a bug!"),
        Encoding::Text => {
            let mut bytes = log
                .get(log_schema().message_key())
                .map(|v| v.as_bytes().to_vec())
                .expect("Failed to encode event as text");
            bytes.push(b'\n');
            bytes
        }
    };

    Some(EncodedEvent::new(
        PartitionInnerBuffer::new(bytes, key.into()),
        byte_size,
    ))
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
    use crate::event::LogEvent;
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AzureBlobSinkConfig>();
    }

    #[test]
    fn azure_blob_encode_event_text() {
        let message = String::from("hello world");
        let log = LogEvent::from(message.clone());
        let blob_prefix = Template::try_from("logs/blob/%F").unwrap();
        let encoding = EncodingConfig {
            codec: Encoding::Text,
            schema: None,
            only_fields: None,
            except_fields: None,
            timestamp_format: None,
        };

        let bytes = encode_event(log.into(), &blob_prefix, &encoding).unwrap();

        let encoded_message = message + "\n";
        let (bytes, _) = bytes.item.into_parts();
        assert_eq!(&bytes[..], encoded_message.as_bytes());
    }

    #[test]
    fn azure_blob_encode_event_json() {
        let message = String::from("hello world");
        let mut log = LogEvent::from(message.clone());
        log.insert("key", "value");
        let blob_prefix = Template::try_from("logs/blob/%F").unwrap();
        let encoding = EncodingConfig {
            codec: Encoding::Ndjson,
            schema: None,
            only_fields: None,
            except_fields: None,
            timestamp_format: None,
        };

        let bytes = encode_event(log.into(), &blob_prefix, &encoding).unwrap();

        let (bytes, _) = bytes.item.into_parts();
        let map: BTreeMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();
        assert_eq!(map[&log_schema().message_key().to_string()], message);
        assert_eq!(map["key"], "value".to_string());
    }

    #[test]
    fn azure_blob_encode_event_with_removed_key() {
        let message = "hello world".to_string();
        let mut log = LogEvent::from(message.clone());
        log.insert("key", "value");
        let blob_prefix = Template::try_from("logs/blob/%F").unwrap();
        let encoding = EncodingConfig {
            codec: Encoding::Ndjson,
            schema: None,
            only_fields: None,
            except_fields: Some(vec!["key".into()]),
            timestamp_format: None,
        };

        let bytes = encode_event(log.into(), &blob_prefix, &encoding).unwrap();

        let (bytes, _) = bytes.item.into_parts();
        let map: BTreeMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();
        assert_eq!(map[&log_schema().message_key().to_string()], message);
        assert!(!map.contains_key("key"));
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
        assert_eq!(request.content_type, "text/plain");
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
        assert_eq!(request.content_type, "application/gzip");
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
        assert_eq!(
            request.blob_name,
            format!("blob{}.log", Utc::now().format("%F"))
        );
        assert_eq!(request.content_encoding, None);
        assert_eq!(request.content_type, "text/plain");
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
        assert_eq!(request.content_type, "text/plain");
    }
}

#[cfg(feature = "azure-blob-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        assert_downcast_matches,
        event::LogEvent,
        test_util::{random_events_with_stream, random_lines, random_lines_with_stream},
    };
    use bytes::{Buf, BytesMut};
    use flate2::read::GzDecoder;
    use futures::Stream;
    use std::io::{BufRead, BufReader};
    use std::num::NonZeroU32;

    #[tokio::test]
    async fn azure_blob_healthcheck_passed() {
        let config = AzureBlobSinkConfig::new_emulator().await;
        let client = config.create_client().expect("Failed to create client");

        let response = config.healthcheck(client).await;

        response.expect("Failed to pass healthcheck");
    }

    #[tokio::test]
    async fn azure_blob_healthcheck_unknown_container() {
        let config = AzureBlobSinkConfig::new_emulator().await;
        let config = AzureBlobSinkConfig {
            container_name: String::from("other-container-name"),
            ..config
        };
        let client = config.create_client().expect("Failed to create client");

        assert_downcast_matches!(
            config.healthcheck(client).await.unwrap_err(),
            HealthcheckError,
            HealthcheckError::UnknownContainer { .. }
        );
    }

    #[tokio::test]
    async fn azure_blob_insert_lines_into_blob() {
        let blob_prefix = String::from("lines/into/blob");
        let config = AzureBlobSinkConfig::new_emulator().await;
        let config = AzureBlobSinkConfig {
            blob_prefix: Some(blob_prefix.clone()),
            ..config
        };
        let sink = config.to_sink();
        let (lines, input) = random_lines_with_stream(100, 10, None);

        sink.run(input).await.expect("Failed to run sink");

        let blobs = config.list_blobs(blob_prefix.as_str()).await;
        assert_eq!(blobs.len(), 1);
        assert!(blobs[0].clone().ends_with(".log"));
        let (blob, blob_lines) = config.get_blob(blobs[0].clone()).await;
        assert_eq!(blob.properties.content_type, String::from("text/plain"));
        assert_eq!(lines, blob_lines);
    }

    #[tokio::test]
    async fn azure_blob_insert_json_into_blob() {
        let blob_prefix = String::from("json/into/blob");
        let config = AzureBlobSinkConfig::new_emulator().await;
        let config = AzureBlobSinkConfig {
            blob_prefix: Some(blob_prefix.clone()),
            encoding: Encoding::Ndjson.into(),
            ..config
        };
        let sink = config.to_sink();
        let (events, input) = random_events_with_stream(100, 10, None);

        sink.run(input).await.expect("Failed to run sink");

        let blobs = config.list_blobs(blob_prefix.as_str()).await;
        assert_eq!(blobs.len(), 1);
        assert!(blobs[0].clone().ends_with(".log"));
        let (blob, blob_lines) = config.get_blob(blobs[0].clone()).await;
        assert_eq!(blob.properties.content_type, String::from("text/plain"));
        let expected = events
            .iter()
            .map(|event| serde_json::to_string(&event.as_log().all_fields()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(expected, blob_lines);
    }

    #[ignore]
    #[tokio::test]
    // This test will fail with Azurite blob emulator because of this issue:
    // https://github.com/Azure/Azurite/issues/629
    async fn azure_blob_insert_lines_into_blob_gzip() {
        let blob_prefix = String::from("lines-gzip/into/blob");
        let config = AzureBlobSinkConfig::new_emulator().await;
        let config = AzureBlobSinkConfig {
            blob_prefix: Some(blob_prefix.clone()),
            compression: Compression::gzip_default(),
            ..config
        };
        let sink = config.to_sink();
        let (lines, events) = random_lines_with_stream(100, 10, None);

        sink.run(events).await.expect("Failed to run sink");

        let blobs = config.list_blobs(blob_prefix.as_str()).await;
        assert_eq!(blobs.len(), 1);
        assert!(blobs[0].clone().ends_with(".log.gz"));
        let (blob, blob_lines) = config.get_blob(blobs[0].clone()).await;
        assert_eq!(
            blob.properties.content_type,
            String::from("application/gzip")
        );
        assert_eq!(lines, blob_lines);
    }

    #[ignore]
    #[tokio::test]
    // This test will fail with Azurite blob emulator because of this issue:
    // https://github.com/Azure/Azurite/issues/629
    async fn azure_blob_insert_json_into_blob_gzip() {
        let blob_prefix = String::from("json-gzip/into/blob");
        let config = AzureBlobSinkConfig::new_emulator().await;
        let config = AzureBlobSinkConfig {
            blob_prefix: Some(blob_prefix.clone()),
            encoding: Encoding::Ndjson.into(),
            compression: Compression::gzip_default(),
            ..config
        };
        let sink = config.to_sink();
        let (events, input) = random_events_with_stream(100, 10, None);

        sink.run(input).await.expect("Failed to run sink");

        let blobs = config.list_blobs(blob_prefix.as_str()).await;
        assert_eq!(blobs.len(), 1);
        assert!(blobs[0].clone().ends_with(".log.gz"));
        let (blob, blob_lines) = config.get_blob(blobs[0].clone()).await;
        assert_eq!(
            blob.properties.content_type,
            String::from("application/gzip")
        );
        let expected = events
            .iter()
            .map(|event| serde_json::to_string(&event.as_log().all_fields()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(expected, blob_lines);
    }

    #[tokio::test]
    async fn azure_blob_rotate_files_after_the_buffer_size_is_reached() {
        let blob_prefix = String::from("lines-rotate/into/blob/");
        let config = AzureBlobSinkConfig::new_emulator().await;
        let config = AzureBlobSinkConfig {
            blob_prefix: Some(blob_prefix.clone() + "{{key}}"),
            blob_append_uuid: Some(false),
            batch: BatchConfig {
                max_bytes: Some(1010),
                ..config.batch
            },
            ..config
        };
        let sink = config.to_sink();
        let groups = 3;
        let (lines, input) = random_lines_with_stream_with_group_key(100, 30, groups);

        sink.run(input).await.expect("Failed to run sink");

        let blobs = config.list_blobs(blob_prefix.as_str()).await;
        assert_eq!(blobs.len(), 3);
        let response = stream::iter(blobs)
            .fold(Vec::new(), |mut acc, blob| async {
                let (_, lines) = config.get_blob(blob).await;
                acc.push(lines);
                acc
            })
            .await;

        for i in 0..groups {
            assert_eq!(&lines[(i * 10)..((i + 1) * 10)], response[i].as_slice());
        }
    }

    impl AzureBlobSinkConfig {
        pub async fn new_emulator() -> AzureBlobSinkConfig {
            let config = AzureBlobSinkConfig {
                connection_string: String::from("UseDevelopmentStorage=true;DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;BlobEndpoint=http://127.0.0.1:10000/devstoreaccount1;QueueEndpoint=http://127.0.0.1:10001/devstoreaccount1;TableEndpoint=http://127.0.0.1:10002/devstoreaccount1;"),
                container_name: "logs".to_string(),
                blob_prefix: None,
                blob_time_format: None,
                blob_append_uuid: None,
                encoding: Encoding::Text.into(),
                compression: Compression::None,
                batch: Default::default(),
                request: TowerRequestConfig::default(),
            };

            config.ensure_container().await;

            config
        }

        pub fn to_sink(&self) -> VectorSink {
            let cx = SinkContext::new_test();
            let client = self.create_client().expect("Failed to create client");

            self.new(client, cx).expect("Failed to create sink")
        }

        pub async fn list_blobs(&self, prefix: &str) -> Vec<String> {
            let client = self.create_client().unwrap();
            let response = client
                .list_blobs()
                .prefix(prefix)
                .max_results(NonZeroU32::new(1000).unwrap())
                .delimiter("/")
                .include_metadata(true)
                .execute()
                .await
                .expect("Failed to fetch blobs");

            let blobs = response
                .blobs
                .blobs
                .iter()
                .map(|blob| blob.name.clone())
                .collect::<Vec<_>>();

            blobs
        }

        pub async fn get_blob(&self, blob: String) -> (Blob, Vec<String>) {
            let client = self.create_client().unwrap();
            let response = client
                .as_blob_client(blob.as_str())
                .get()
                .range(Range::new(0, 1024 * 1024))
                .execute()
                .await
                .expect("Failed to get blob");

            (response.blob, self.get_blob_content(response.data.to_vec()))
        }

        fn get_blob_content(&self, data: Vec<u8>) -> Vec<String> {
            let body = BytesMut::from(data.as_slice()).freeze().reader();

            if self.compression == Compression::None {
                BufReader::new(body).lines().map(|l| l.unwrap()).collect()
            } else {
                BufReader::new(GzDecoder::new(body))
                    .lines()
                    .map(|l| l.unwrap())
                    .collect()
            }
        }

        async fn ensure_container(&self) {
            let client = self.create_client().unwrap();
            let request = client.create().public_access(PublicAccess::None).execute();

            let response = match request.await {
                Ok(_) => Ok(()),
                Err(reason) => match reason.downcast_ref::<HttpError>() {
                    Some(HttpError::UnexpectedStatusCode { received, .. }) => match *received {
                        StatusCode::CONFLICT => Ok(()),
                        status => Err(format!("Unexpected status code {}", status)),
                    },
                    _ => Err(format!("Unexpected error {}", reason.to_string())),
                },
            };

            response.expect("Failed to create container")
        }
    }

    fn random_lines_with_stream_with_group_key(
        len: usize,
        count: usize,
        groups: usize,
    ) -> (Vec<String>, impl Stream<Item = Event>) {
        let key = count / groups;
        let lines = random_lines(len).take(count).collect::<Vec<_>>();
        let events = lines.clone().into_iter().enumerate().map(move |(i, line)| {
            let mut log = LogEvent::from(line);
            let i = ((i / key) + 1) as i32;
            log.insert("key", i);
            log.into()
        });

        (lines, stream::iter(events))
    }
}
