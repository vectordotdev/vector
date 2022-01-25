use std::{convert::TryInto, io, sync::Arc};

use azure_storage_blobs::prelude::*;
use bytes::Bytes;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use uuid::Uuid;
use vector_core::ByteSizeOf;

use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext},
    event::{Event, Finalizable},
    sinks::{
        azure_common::{
            self,
            config::{AzureBlobMetadata, AzureBlobRequest, AzureBlobRetryLogic},
            service::AzureBlobService,
            sink::AzureBlobSink,
        },
        util::{
            encoding::{EncodingConfig, StandardEncodings},
            partitioner::KeyPartitioner,
            BatchConfig, BulkSizeBasedDefaultBatchSettings, Compression, RequestBuilder,
            ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    Result,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct AzureBlobSinkConfig {
    pub connection_string: String,
    pub container_name: String,
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

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "azure_blob"
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

#[derive(Clone)]
pub struct AzureBlobRequestOptions {
    pub container_name: String,
    pub blob_time_format: String,
    pub blob_append_uuid: bool,
    pub encoding: EncodingConfig<StandardEncodings>,
    pub compression: Compression,
}

impl RequestBuilder<(String, Vec<Event>)> for AzureBlobRequestOptions {
    type Metadata = AzureBlobMetadata;
    type Events = Vec<Event>;
    type Encoder = EncodingConfig<StandardEncodings>;
    type Payload = Bytes;
    type Request = AzureBlobRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
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
        let blob_name = {
            let formatted_ts = Utc::now().format(self.blob_time_format.as_str());

            self.blob_append_uuid
                .then(|| format!("{}-{}", formatted_ts, Uuid::new_v4().to_hyphenated()))
                .unwrap_or_else(|| formatted_ts.to_string())
        };

        let extension = self.compression.extension();
        metadata.partition_key = format!("{}{}.{}", metadata.partition_key, blob_name, extension);

        debug!(
            message = "Sending events.",
            bytes = ?payload.len(),
            events_len = ?metadata.count,
            blob = ?metadata.partition_key,
            container = ?self.container_name,
        );

        AzureBlobRequest {
            blob_data: payload,
            content_encoding: self.compression.content_encoding(),
            content_type: self.compression.content_type(),
            metadata,
        }
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

#[cfg(test)]
fn default_config(e: StandardEncodings) -> AzureBlobSinkConfig {
    AzureBlobSinkConfig {
        connection_string: Default::default(),
        container_name: Default::default(),
        blob_prefix: Default::default(),
        blob_time_format: Default::default(),
        blob_append_uuid: Default::default(),
        encoding: e.into(),
        compression: Compression::gzip_default(),
        batch: Default::default(),
        request: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use vector_core::partition::Partitioner;

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AzureBlobSinkConfig>();
    }

    #[test]
    fn azure_blob_build_request_without_compression() {
        let log = Event::from("test message");
        let compression = Compression::None;
        let container_name = String::from("logs");
        let sink_config = AzureBlobSinkConfig {
            blob_prefix: Some("blob".into()),
            container_name: container_name.clone(),
            ..default_config(StandardEncodings::Text)
        };
        let blob_time_format = String::from("");
        let blob_append_uuid = false;

        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&log)
            .expect("key wasn't provided");

        let request_options = AzureBlobRequestOptions {
            container_name,
            blob_time_format,
            blob_append_uuid,
            encoding: StandardEncodings::Text.into(),
            compression,
        };

        let (metadata, _events) = request_options.split_input((key, vec![log]));
        let request = request_options.build_request(metadata, Bytes::new());

        assert_eq!(request.metadata.partition_key, "blob.log".to_string());
        assert_eq!(request.content_encoding, None);
        assert_eq!(request.content_type, "text/plain");
    }

    #[test]
    fn azure_blob_build_request_with_compression() {
        let log = Event::from("test message");
        let compression = Compression::gzip_default();
        let container_name = String::from("logs");
        let sink_config = AzureBlobSinkConfig {
            blob_prefix: Some("blob".into()),
            container_name: container_name.clone(),
            ..default_config(StandardEncodings::Text)
        };
        let blob_time_format = String::from("");
        let blob_append_uuid = false;

        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&log)
            .expect("key wasn't provided");

        let request_options = AzureBlobRequestOptions {
            container_name,
            blob_time_format,
            blob_append_uuid,
            encoding: StandardEncodings::Text.into(),
            compression,
        };

        let (metadata, _events) = request_options.split_input((key, vec![log]));
        let request = request_options.build_request(metadata, Bytes::new());

        assert_eq!(request.metadata.partition_key, "blob.log.gz".to_string());
        assert_eq!(request.content_encoding, Some("gzip"));
        assert_eq!(request.content_type, "application/gzip");
    }

    #[test]
    fn azure_blob_build_request_with_time_format() {
        let log = Event::from("test message");
        let compression = Compression::None;
        let container_name = String::from("logs");
        let sink_config = AzureBlobSinkConfig {
            blob_prefix: Some("blob".into()),
            container_name: container_name.clone(),
            ..default_config(StandardEncodings::Text)
        };
        let blob_time_format = String::from("%F");
        let blob_append_uuid = false;

        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&log)
            .expect("key wasn't provided");

        let request_options = AzureBlobRequestOptions {
            container_name,
            blob_time_format,
            blob_append_uuid,
            encoding: StandardEncodings::Text.into(),
            compression,
        };

        let (metadata, _events) = request_options.split_input((key, vec![log]));
        let request = request_options.build_request(metadata, Bytes::new());

        assert_eq!(
            request.metadata.partition_key,
            format!("blob{}.log", Utc::now().format("%F"))
        );
        assert_eq!(request.content_encoding, None);
        assert_eq!(request.content_type, "text/plain");
    }

    #[test]
    fn azure_blob_build_request_with_uuid() {
        let log = Event::from("test message");
        let compression = Compression::None;
        let container_name = String::from("logs");
        let sink_config = AzureBlobSinkConfig {
            blob_prefix: Some("blob".into()),
            container_name: container_name.clone(),
            ..default_config(StandardEncodings::Text)
        };
        let blob_time_format = String::from("");
        let blob_append_uuid = true;

        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&log)
            .expect("key wasn't provided");

        let request_options = AzureBlobRequestOptions {
            container_name,
            blob_time_format,
            blob_append_uuid,
            encoding: StandardEncodings::Text.into(),
            compression,
        };

        let (metadata, _events) = request_options.split_input((key, vec![log]));
        let request = request_options.build_request(metadata, Bytes::new());

        assert_ne!(request.metadata.partition_key, "blob.log".to_string());
        assert_eq!(request.content_encoding, None);
        assert_eq!(request.content_type, "text/plain");
    }
}

#[cfg(feature = "azure-blob-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use std::{
        io::{BufRead, BufReader},
        num::NonZeroU32,
    };

    use azure_core::{prelude::Range, HttpError};
    use bytes::{Buf, BytesMut};
    use flate2::read::GzDecoder;
    use futures::{stream, Stream, StreamExt};
    use http::StatusCode;

    use super::*;
    use crate::{
        event::{EventArray, LogEvent},
        test_util::{random_events_with_stream, random_lines, random_lines_with_stream},
    };

    #[tokio::test]
    async fn azure_blob_healthcheck_passed() {
        let config = AzureBlobSinkConfig::new_emulator().await;
        let client = azure_common::config::build_client(
            config.connection_string,
            config.container_name.clone(),
        )
        .expect("Failed to create client");

        let response = azure_common::config::build_healthcheck(config.container_name, client);

        response.expect("Failed to pass healthcheck");
    }

    #[tokio::test]
    async fn azure_blob_healthcheck_unknown_container() {
        let config = AzureBlobSinkConfig::new_emulator().await;
        let config = AzureBlobSinkConfig {
            container_name: String::from("other-container-name"),
            ..config
        };
        let client = azure_common::config::build_client(
            config.connection_string,
            config.container_name.clone(),
        )
        .expect("Failed to create client");

        assert_eq!(
            azure_common::config::build_healthcheck(config.container_name, client)
                .unwrap()
                .await
                .unwrap_err()
                .to_string(),
            "Container: \"other-container-name\" not found"
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
            encoding: StandardEncodings::Ndjson.into(),
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
            encoding: StandardEncodings::Ndjson.into(),
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
        let groups = 3;
        let (lines, size, input) = random_lines_with_stream_with_group_key(100, 30, groups);
        let size_per_group = (size / groups) + 10;

        let blob_prefix = String::from("lines-rotate/into/blob/");
        let mut config = AzureBlobSinkConfig::new_emulator().await;
        config.batch.max_bytes = Some(size_per_group);

        let config = AzureBlobSinkConfig {
            blob_prefix: Some(blob_prefix.clone() + "{{key}}"),
            blob_append_uuid: Some(false),
            batch: config.batch,
            ..config
        };

        let sink = config.to_sink();
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
            let address = std::env::var("AZURE_ADDRESS").unwrap_or_else(|_| "localhost".into());
            let config = AzureBlobSinkConfig {
                connection_string: format!("UseDevelopmentStorage=true;DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;BlobEndpoint=http://{}:10000/devstoreaccount1;QueueEndpoint=http://{}:10001/devstoreaccount1;TableEndpoint=http://{}:10002/devstoreaccount1;", address, address, address),
                container_name: "logs".to_string(),
                blob_prefix: None,
                blob_time_format: None,
                blob_append_uuid: None,
                encoding: StandardEncodings::Text.into(),
                compression: Compression::None,
                batch: Default::default(),
                request: TowerRequestConfig::default(),
            };

            config.ensure_container().await;

            config
        }

        pub fn to_sink(&self) -> VectorSink {
            let cx = SinkContext::new_test();
            let client = azure_common::config::build_client(
                self.connection_string.clone(),
                self.container_name.clone(),
            )
            .expect("Failed to create client");

            self.build_processor(client, cx)
                .expect("Failed to create sink")
        }

        pub async fn list_blobs(&self, prefix: &str) -> Vec<String> {
            let client = azure_common::config::build_client(
                self.connection_string.clone(),
                self.container_name.clone(),
            )
            .unwrap();
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
            let client = azure_common::config::build_client(
                self.connection_string.clone(),
                self.container_name.clone(),
            )
            .unwrap();
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
            let client = azure_common::config::build_client(
                self.connection_string.clone(),
                self.container_name.clone(),
            )
            .unwrap();
            let request = client.create().public_access(PublicAccess::None).execute();

            let response = match request.await {
                Ok(_) => Ok(()),
                Err(reason) => match reason.downcast_ref::<HttpError>() {
                    Some(HttpError::StatusCode { status, .. }) => match *status {
                        StatusCode::CONFLICT => Ok(()),
                        status => Err(format!("Unexpected status code {}", status)),
                    },
                    _ => Err(format!("Unexpected error {}", reason)),
                },
            };

            response.expect("Failed to create container")
        }
    }

    fn random_lines_with_stream_with_group_key(
        len: usize,
        count: usize,
        groups: usize,
    ) -> (Vec<String>, usize, impl Stream<Item = EventArray>) {
        let key = count / groups;
        let lines = random_lines(len).take(count).collect::<Vec<_>>();
        let (size, events) = lines
            .clone()
            .into_iter()
            .enumerate()
            .map(move |(i, line)| {
                let mut log = LogEvent::from(line);
                let i = ((i / key) + 1) as i32;
                log.insert("key", i);
                Event::from(log)
            })
            .fold((0, Vec::new()), |(mut size, mut events), event| {
                size += event.size_of();
                events.push(event.into());
                (size, events)
            });

        (lines, size, stream::iter(events))
    }
}
