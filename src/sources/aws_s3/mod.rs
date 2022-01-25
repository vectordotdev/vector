use std::convert::TryInto;

use async_compression::tokio::bufread;
use futures::{stream, stream::StreamExt};
use rusoto_core::Region;
use rusoto_s3::S3Client;
use rusoto_sqs::SqsClient;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

use super::util::MultilineConfig;
use crate::{
    aws::{
        auth::AwsAuthentication,
        rusoto::{self, RegionOrEndpoint},
    },
    config::{
        AcknowledgementsConfig, DataType, Output, ProxyConfig, SourceConfig, SourceContext,
        SourceDescription,
    },
    line_agg,
    serde::bool_or_struct,
};

pub mod sqs;

#[derive(Derivative, Copy, Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
pub enum Compression {
    #[derivative(Default)]
    Auto,
    None,
    Gzip,
    Zstd,
}

#[derive(Derivative, Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
enum Strategy {
    #[derivative(Default)]
    Sqs,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct AwsS3Config {
    #[serde(flatten)]
    region: RegionOrEndpoint,

    compression: Compression,

    strategy: Strategy,

    sqs: Option<sqs::Config>,

    // Deprecated name. Moved to auth.
    assume_role: Option<String>,
    #[serde(default)]
    auth: AwsAuthentication,

    multiline: Option<MultilineConfig>,

    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
}

inventory::submit! {
    SourceDescription::new::<AwsS3Config>("aws_s3")
}

impl_generate_config_from_default!(AwsS3Config);

#[async_trait::async_trait]
#[typetag::serde(name = "aws_s3")]
impl SourceConfig for AwsS3Config {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let multiline_config: Option<line_agg::Config> = self
            .multiline
            .as_ref()
            .map(|config| config.try_into())
            .transpose()?;
        let acknowledgements = cx.globals.acknowledgements.merge(&self.acknowledgements);

        match self.strategy {
            Strategy::Sqs => Ok(Box::pin(
                self.create_sqs_ingestor(multiline_config, &cx.proxy)
                    .await?
                    .run(cx, acknowledgements),
            )),
        }
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "aws_s3"
    }
}

impl AwsS3Config {
    async fn create_sqs_ingestor(
        &self,
        multiline: Option<line_agg::Config>,
        proxy: &ProxyConfig,
    ) -> Result<sqs::Ingestor, CreateSqsIngestorError> {
        use std::sync::Arc;

        let region: Region = (&self.region).try_into().context(RegionParseSnafu {})?;

        let client = rusoto::client(proxy).with_context(|_| ClientSnafu {})?;
        let creds: Arc<rusoto::AwsCredentialsProvider> = self
            .auth
            .build(&region, self.assume_role.clone())
            .context(CredentialsSnafu {})?
            .into();
        let s3_client = S3Client::new_with(
            client.clone(),
            Arc::<rusoto::AwsCredentialsProvider>::clone(&creds),
            region.clone(),
        );

        match self.sqs {
            Some(ref sqs) => {
                let sqs_client = SqsClient::new_with(
                    client.clone(),
                    Arc::<rusoto::AwsCredentialsProvider>::clone(&creds),
                    region.clone(),
                );

                sqs::Ingestor::new(
                    region.clone(),
                    sqs_client,
                    s3_client,
                    sqs.clone(),
                    self.compression,
                    multiline,
                )
                .await
                .context(InitializeSnafu {})
            }
            None => Err(CreateSqsIngestorError::ConfigMissing {}),
        }
    }
}

#[derive(Debug, Snafu)]
enum CreateSqsIngestorError {
    #[snafu(display("Unable to initialize: {}", source))]
    Initialize { source: sqs::IngestorNewError },
    #[snafu(display("Unable to create AWS client: {}", source))]
    Client { source: crate::Error },
    #[snafu(display("Unable to create AWS credentials provider: {}", source))]
    Credentials { source: crate::Error },
    #[snafu(display("Configuration for `sqs` required when strategy=sqs"))]
    ConfigMissing,
    #[snafu(display("Could not parse region configuration: {}", source))]
    RegionParse { source: rusoto::region::ParseError },
}

/// None if body is empty
async fn s3_object_decoder(
    compression: Compression,
    key: &str,
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    mut body: rusoto_s3::StreamingBody,
) -> Box<dyn tokio::io::AsyncRead + Send + Unpin> {
    let first = if let Some(first) = body.next().await {
        first
    } else {
        return Box::new(tokio::io::empty());
    };

    let r = tokio::io::BufReader::new(
        rusoto_s3::StreamingBody::new(stream::iter(Some(first)).chain(body)).into_async_read(),
    );

    let compression = match compression {
        Auto => {
            determine_compression(content_encoding, content_type, key).unwrap_or(Compression::None)
        }
        _ => compression,
    };

    use Compression::*;
    match compression {
        Auto => unreachable!(), // is mapped above
        None => Box::new(r),
        Gzip => Box::new({
            let mut decoder = bufread::GzipDecoder::new(r);
            decoder.multiple_members(true);
            decoder
        }),
        Zstd => Box::new({
            let mut decoder = bufread::ZstdDecoder::new(r);
            decoder.multiple_members(true);
            decoder
        }),
    }
}

/// try to determine the compression given the:
/// * content-encoding
/// * content-type
/// * key name (for file extension)
///
/// It will use this information in this order
fn determine_compression(
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    key: &str,
) -> Option<Compression> {
    content_encoding
        .and_then(content_encoding_to_compression)
        .or_else(|| content_type.and_then(content_type_to_compression))
        .or_else(|| object_key_to_compression(key))
}

fn content_encoding_to_compression(content_encoding: &str) -> Option<Compression> {
    match content_encoding {
        "gzip" => Some(Compression::Gzip),
        "zstd" => Some(Compression::Zstd),
        _ => None,
    }
}

fn content_type_to_compression(content_type: &str) -> Option<Compression> {
    match content_type {
        "application/gzip" | "application/x-gzip" => Some(Compression::Gzip),
        "application/zstd" => Some(Compression::Zstd),
        _ => None,
    }
}

fn object_key_to_compression(key: &str) -> Option<Compression> {
    let extension = std::path::Path::new(key)
        .extension()
        .and_then(std::ffi::OsStr::to_str);

    use Compression::*;
    extension.and_then(|extension| match extension {
        "gz" => Some(Gzip),
        "zst" => Some(Zstd),
        _ => Option::None,
    })
}

#[cfg(test)]
mod test {
    use tokio::io::AsyncReadExt;

    use super::{s3_object_decoder, Compression};

    #[test]
    fn determine_compression() {
        use super::Compression;

        let cases = vec![
            ("out.log", Some("gzip"), None, Some(Compression::Gzip)),
            (
                "out.log",
                None,
                Some("application/gzip"),
                Some(Compression::Gzip),
            ),
            ("out.log.gz", None, None, Some(Compression::Gzip)),
            ("out.txt", None, None, None),
        ];
        for case in cases {
            let (key, content_encoding, content_type, expected) = case;
            assert_eq!(
                super::determine_compression(content_encoding, content_type, key),
                expected,
                "key={:?} content_encoding={:?} content_type={:?}",
                key,
                content_encoding,
                content_type,
            );
        }
    }

    #[tokio::test]
    async fn decode_empty_message_gzip() {
        let key = uuid::Uuid::new_v4().to_string();

        let mut data = Vec::new();
        s3_object_decoder(
            Compression::Auto,
            &key,
            Some("gzip"),
            None,
            rusoto_s3::StreamingBody::new(futures::stream::empty()),
        )
        .await
        .read_to_end(&mut data)
        .await
        .unwrap();

        assert!(data.is_empty());
    }
}

#[cfg(feature = "aws-s3-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use pretty_assertions::assert_eq;
    use rusoto_core::Region;
    use rusoto_s3::{PutObjectRequest, S3Client, S3};
    use rusoto_sqs::{Sqs, SqsClient};

    use super::{sqs, AwsS3Config, Compression, Strategy};
    use crate::{
        aws::rusoto::RegionOrEndpoint,
        config::{SourceConfig, SourceContext},
        event::EventStatus::{self, *},
        line_agg,
        sources::util::MultilineConfig,
        test_util::{
            collect_n, lines_from_gzip_file, lines_from_zst_file, random_lines, trace_init,
        },
        SourceSender,
    };

    #[tokio::test]
    async fn s3_process_message() {
        trace_init();

        let logs: Vec<String> = random_lines(100).take(10).collect();

        test_event(
            None,
            None,
            None,
            None,
            logs.join("\n").into_bytes(),
            logs,
            Delivered,
        )
        .await;
    }

    #[tokio::test]
    async fn s3_process_message_special_characters() {
        trace_init();

        let key = format!("special:{}", uuid::Uuid::new_v4());
        let logs: Vec<String> = random_lines(100).take(10).collect();

        test_event(
            Some(key),
            None,
            None,
            None,
            logs.join("\n").into_bytes(),
            logs,
            Delivered,
        )
        .await;
    }

    #[tokio::test]
    async fn s3_process_message_gzip() {
        use std::io::Read;

        trace_init();

        let logs: Vec<String> = random_lines(100).take(10).collect();

        let mut gz = flate2::read::GzEncoder::new(
            std::io::Cursor::new(logs.join("\n").into_bytes()),
            flate2::Compression::fast(),
        );
        let mut buffer = Vec::new();
        gz.read_to_end(&mut buffer).unwrap();

        test_event(None, Some("gzip"), None, None, buffer, logs, Delivered).await;
    }

    #[tokio::test]
    async fn s3_process_message_multipart_gzip() {
        use std::io::Read;

        trace_init();

        let logs = lines_from_gzip_file("tests/data/multipart-gzip.log.gz");

        let buffer = {
            let mut file = std::fs::File::open("tests/data/multipart-gzip.log.gz")
                .expect("file can be opened");
            let mut data = Vec::new();
            file.read_to_end(&mut data).expect("file can be read");
            data
        };

        test_event(None, Some("gzip"), None, None, buffer, logs, Delivered).await;
    }

    #[tokio::test]
    async fn s3_process_message_multipart_zstd() {
        use std::io::Read;

        trace_init();

        let logs = lines_from_zst_file("tests/data/multipart-zst.log.zst");

        let buffer = {
            let mut file = std::fs::File::open("tests/data/multipart-zst.log.zst")
                .expect("file can be opened");
            let mut data = Vec::new();
            file.read_to_end(&mut data).expect("file can be read");
            data
        };

        test_event(None, Some("zstd"), None, None, buffer, logs, Delivered).await;
    }

    #[tokio::test]
    async fn s3_process_message_multiline() {
        trace_init();

        let logs: Vec<String> = vec!["abc", "def", "geh"]
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();

        test_event(
            None,
            None,
            None,
            Some(MultilineConfig {
                start_pattern: "abc".to_owned(),
                mode: line_agg::Mode::HaltWith,
                condition_pattern: "geh".to_owned(),
                timeout_ms: 1000,
            }),
            logs.join("\n").into_bytes(),
            vec!["abc\ndef\ngeh".to_owned()],
            Delivered,
        )
        .await;
    }

    #[tokio::test]
    async fn handles_errored_status() {
        trace_init();

        let logs: Vec<String> = random_lines(100).take(10).collect();

        test_event(
            None,
            None,
            None,
            None,
            logs.join("\n").into_bytes(),
            logs,
            Errored,
        )
        .await;
    }

    #[tokio::test]
    async fn handles_failed_status() {
        trace_init();

        let logs: Vec<String> = random_lines(100).take(10).collect();

        test_event(
            None,
            None,
            None,
            None,
            logs.join("\n").into_bytes(),
            logs,
            Rejected,
        )
        .await;
    }

    fn s3_address() -> String {
        std::env::var("S3_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
    }

    fn config(queue_url: &str, multiline: Option<MultilineConfig>) -> AwsS3Config {
        AwsS3Config {
            region: RegionOrEndpoint::with_endpoint(s3_address()),
            strategy: Strategy::Sqs,
            compression: Compression::Auto,
            multiline,
            sqs: Some(sqs::Config {
                queue_url: queue_url.to_string(),
                poll_secs: 1,
                visibility_timeout_secs: 0,
                client_concurrency: 1,
                ..Default::default()
            }),
            acknowledgements: true.into(),
            ..Default::default()
        }
    }

    // puts an object and asserts that the logs it gets back match
    async fn test_event(
        key: Option<String>,
        content_encoding: Option<&str>,
        content_type: Option<&str>,
        multiline: Option<MultilineConfig>,
        payload: Vec<u8>,
        expected_lines: Vec<String>,
        status: EventStatus,
    ) {
        let key = key.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let s3 = s3_client();
        let sqs = sqs_client();

        let queue = create_queue(&sqs).await;
        let bucket = create_bucket(&s3, &queue).await;

        let config = config(&queue, multiline);

        s3.put_object(PutObjectRequest {
            bucket: bucket.clone(),
            key: key.clone(),
            body: Some(rusoto_core::ByteStream::from(payload)),
            content_type: content_type.map(|t| t.to_owned()),
            content_encoding: content_encoding.map(|t| t.to_owned()),
            ..Default::default()
        })
        .await
        .expect("Could not put object");

        assert_eq!(count_messages(&sqs, &queue).await, 1);

        let (tx, rx) = SourceSender::new_test_finalize(status);
        let cx = SourceContext::new_test(tx);
        let source = config.build(cx).await.unwrap();
        tokio::spawn(async move { source.await.unwrap() });

        let events = collect_n(rx, expected_lines.len()).await;

        assert_eq!(expected_lines.len(), events.len());
        for (i, event) in events.iter().enumerate() {
            let message = expected_lines[i].as_str();

            let log = event.as_log();
            assert_eq!(log["message"], message.into());
            assert_eq!(log["bucket"], bucket.clone().into());
            assert_eq!(log["object"], key.clone().into());
            assert_eq!(log["region"], "us-east-1".into());
        }

        // Make sure the SQS message is deleted
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let expected_messages = match status {
            Errored => 1,
            _ => 0,
        };
        assert_eq!(count_messages(&sqs, &queue).await, expected_messages);
    }

    /// creates a new SQS queue
    ///
    /// returns the queue name
    async fn create_queue(client: &SqsClient) -> String {
        use rusoto_sqs::CreateQueueRequest;

        let queue_name = uuid::Uuid::new_v4().to_string();

        let res = client
            .create_queue(CreateQueueRequest {
                queue_name: queue_name.clone(),
                ..Default::default()
            })
            .await
            .expect("Could not create queue");

        res.queue_url.expect("no queue url")
    }

    /// count the number of messages in a SQS queue
    async fn count_messages(client: &SqsClient, queue: &str) -> usize {
        client
            .receive_message(rusoto_sqs::ReceiveMessageRequest {
                queue_url: queue.into(),
                visibility_timeout: Some(0),
                ..Default::default()
            })
            .await
            .unwrap()
            .messages
            .map(|messages| messages.len())
            .unwrap_or(0)
    }

    /// creates a new bucket with notifications to given SQS queue
    ///
    /// returns the bucket name
    async fn create_bucket(client: &S3Client, queue_name: &str) -> String {
        use rusoto_s3::{
            CreateBucketRequest, NotificationConfiguration,
            PutBucketNotificationConfigurationRequest, QueueConfiguration,
        };

        let bucket_name = uuid::Uuid::new_v4().to_string();

        client
            .create_bucket(CreateBucketRequest {
                bucket: bucket_name.clone(),
                ..Default::default()
            })
            .await
            .expect("Could not create bucket");

        client
            .put_bucket_notification_configuration(PutBucketNotificationConfigurationRequest {
                bucket: bucket_name.clone(),
                expected_bucket_owner: None,
                notification_configuration: NotificationConfiguration {
                    queue_configurations: Some(vec![QueueConfiguration {
                        events: vec!["s3:ObjectCreated:*".to_string()],
                        queue_arn: format!("arn:aws:sqs:us-east-1:000000000000:{}", queue_name),
                        ..Default::default()
                    }]),
                    ..Default::default()
                },
            })
            .await
            .expect("Could not create bucket notification");

        bucket_name
    }

    fn s3_client() -> S3Client {
        let region = Region::Custom {
            name: "minio".to_owned(),
            endpoint: s3_address(),
        };

        S3Client::new(region)
    }

    fn sqs_client() -> SqsClient {
        let region = Region::Custom {
            name: "minio".to_owned(),
            endpoint: s3_address(),
        };

        SqsClient::new(region)
    }
}
