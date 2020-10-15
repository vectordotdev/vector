use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    dns::Resolver,
    event::Event,
    shutdown::ShutdownSignal,
    sinks::util::rusoto,
    Pipeline,
};
use codec::BytesDelimitedCodec;
use futures::{
    compat::{Compat, Future01CompatExt},
    future::{FutureExt, TryFutureExt},
    stream::StreamExt,
};
use futures01::Sink;
use rusoto_core::{Region, RusotoError};
use rusoto_s3::{GetObjectRequest, S3Client, S3};
use rusoto_sqs::{
    DeleteMessageError, DeleteMessageRequest, GetQueueUrlError, GetQueueUrlRequest, Message,
    ReceiveMessageError, ReceiveMessageRequest, Sqs, SqsClient,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{convert::TryInto, time::Duration};
use tokio::{select, time};
use tokio_util::codec::FramedRead;

// TODO:
// * Handle decompression
// * Revisit configuration of queue. Should we take the URL instead?
//   * At the least, support setting a differente queue owner
// * Revisit configuration of SQS strategy (intrnal vs. external tagging)
// * Move AWS utils from sink to general
// * Use multiline config
// * Max line bytes
// * Make sure we are handling shutdown well
// * Consider any special handling of FIFO SQS queues
// * Consider having helper methods stream data and have top-level forward to pipeline
// * Integration tests
// * Consider / decide on multi-region S3 support (handling messages referring to buckets in
//   multiple regions)
// * Consider / decide on custom endpoint support
//   * How would we handle this for multi-region S3 support?
//
// Future work:
// * Additional codecs. Just treating like `file` source with newlines for now

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Compression {
    Auto,
    None,
    Gzip,
    Lz4,
    Snappy,
    Zstd,
}

impl Default for Compression {
    fn default() -> Self {
        Compression::Auto
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Strategy {
    Sqs,
}

impl Default for Strategy {
    fn default() -> Self {
        Strategy::Sqs
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AwsS3Config {
    #[serde(default)]
    compression: Compression,

    #[serde(default)]
    strategy: Strategy,

    #[serde(default)]
    sqs: Option<SqsConfig>,

    #[serde(default)]
    assume_role: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SqsConfig {
    region: Region,
    queue_name: String,
    #[serde(default = "default_poll_interval_secs")]
    poll_secs: u64,
    #[serde(default = "default_visibility_timeout_secs")]
    visibility_timeout_secs: u64,
    #[serde(default = "default_true")]
    delete_message: bool,
}

const fn default_poll_interval_secs() -> u64 {
    15
}

const fn default_visibility_timeout_secs() -> u64 {
    300
}
const fn default_true() -> bool {
    true
}

inventory::submit! {
    SourceDescription::new::<AwsS3Config>("aws_s3")
}

impl_generate_config_from_default!(AwsS3Config);

#[async_trait::async_trait]
#[typetag::serde(name = "aws_s3")]
impl SourceConfig for AwsS3Config {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        match self.strategy {
            Strategy::Sqs => Ok(Box::new(
                self.create_sqs_ingestor()
                    .await?
                    .run(out, shutdown)
                    .boxed()
                    .compat(),
            )),
        }
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "aws_s3"
    }
}

#[derive(Debug, Snafu)]
enum CreateSqsIngestorError {
    #[snafu(display("Unable to initialize: {}", source))]
    Initialize { source: SqsIngestorNewError },
    #[snafu(display("Unable to create AWS client: {}", source))]
    Client { source: crate::Error },
    #[snafu(display("Unable to create AWS credentials provider: {}", source))]
    Credentials { source: crate::Error },
    #[snafu(display("sqs configuration required when strategy=sqs"))]
    ConfigMissing,
}

impl AwsS3Config {
    async fn create_sqs_ingestor(&self) -> Result<SqsIngestor, CreateSqsIngestorError> {
        match self.sqs {
            Some(ref sqs) => {
                // TODO:
                // * move resolver?
                // * try cloning credentials provider again?
                let resolver = Resolver;
                let client = rusoto::client(resolver).with_context(|| Client {})?;
                let creds =
                    rusoto::AwsCredentialsProvider::new(&sqs.region, self.assume_role.clone())
                        .with_context(|| Credentials {})?;
                let sqs_client = SqsClient::new_with(client.clone(), creds, sqs.region.clone());
                let creds =
                    rusoto::AwsCredentialsProvider::new(&sqs.region, self.assume_role.clone())
                        .with_context(|| Credentials {})?;
                let s3_client = S3Client::new_with(client.clone(), creds, sqs.region.clone());

                SqsIngestor::new(sqs_client, s3_client, sqs.clone(), self.compression)
                    .await
                    .with_context(|| Initialize {})
            }
            None => Err(CreateSqsIngestorError::ConfigMissing {}),
        }
    }
}

#[derive(Debug, Snafu)]
enum SqsIngestorNewError {
    #[snafu(display("Unable to fetch queue URL for {}: {}", name, source))]
    FetchQueueUrl {
        source: RusotoError<GetQueueUrlError>,
        name: String,
    },
    #[snafu(display("Got an empty queue URL for {}", name))]
    MissingQueueUrl { name: String },
}

struct SqsIngestor {
    s3_client: S3Client,
    sqs_client: SqsClient,

    compression: Compression,

    queue_url: String,
    poll_interval: Duration,
    visibility_timeout: Duration,
    delete_message: bool,
}

impl SqsIngestor {
    async fn new(
        sqs_client: SqsClient,
        s3_client: S3Client,
        config: SqsConfig,
        compression: Compression,
    ) -> Result<SqsIngestor, SqsIngestorNewError> {
        let queue_url_result = sqs_client
            .get_queue_url(GetQueueUrlRequest {
                queue_name: config.queue_name.clone(),
                ..Default::default()
            })
            .await
            .with_context(|| FetchQueueUrl {
                name: config.queue_name.clone(),
            })?;

        let queue_url = queue_url_result
            .queue_url
            .ok_or(SqsIngestorNewError::MissingQueueUrl {
                name: config.queue_name.clone(),
            })?;

        Ok(SqsIngestor {
            s3_client,
            sqs_client,

            compression,

            queue_url,
            poll_interval: Duration::from_secs(config.poll_secs),
            visibility_timeout: Duration::from_secs(config.visibility_timeout_secs),
            delete_message: config.delete_message,
        })
    }

    async fn run(self, out: Pipeline, mut shutdown: ShutdownSignal) -> Result<(), ()> {
        let mut interval = time::interval(self.poll_interval).map(|_| ());

        loop {
            select! {
                Some(()) = interval.next() => (),
                _ = &mut shutdown => break Ok(()),
                else => break Ok(()),
            };

            let messages = self.receive_messages().await.unwrap_or_default();

            for message in messages {
                let receipt_handle = message.receipt_handle.clone();

                match self.handle_sqs_message(message, out.clone()).await {
                    Ok(()) => {
                        if self.delete_message {
                            self.delete_message(receipt_handle).await.unwrap();
                        }
                    }
                    Err(()) => {} // TODO emit error
                }
            }
        }
    }

    async fn handle_sqs_message(&self, message: Message, out: Pipeline) -> Result<(), ()> {
        let s3_event: S3Event =
            serde_json::from_str(message.body.unwrap_or_default().as_ref()).unwrap();

        self.handle_s3_event(s3_event, out).map_ok(|_| ()).await
    }

    async fn handle_s3_event(&self, s3_event: S3Event, out: Pipeline) -> Result<(), ()> {
        for record in s3_event.records {
            self.handle_s3_event_record(record, out.clone()).await?
        }
        Ok(())
    }

    async fn handle_s3_event_record(
        &self,
        s3_event: S3EventRecord,
        out: Pipeline,
    ) -> Result<(), ()> {
        let object = self
            .s3_client
            .get_object(GetObjectRequest {
                bucket: s3_event.s3.bucket.name.clone(),
                key: s3_event.s3.object.key.clone(),
                ..Default::default()
            })
            .await
            .unwrap();

        // TODO assert event type

        let metadata = object.metadata.unwrap_or_default().clone(); // TODO can we avoid cloning the hashmap?

        match object.body {
            Some(body) => {
                let stream = FramedRead::new(
                    body.into_async_read(),
                    BytesDelimitedCodec::new_with_max_length(b'\n', 100000),
                )
                .filter_map(|line| {
                    let bucket_name = s3_event.s3.bucket.name.clone();
                    let object_key = s3_event.s3.object.key.clone();
                    let aws_region = s3_event.aws_region.clone();
                    let metadata = metadata.clone();

                    async move {
                        match line {
                            Ok(line) => {
                                let mut event = Event::from(line);

                                let log = event.as_mut_log();
                                log.insert("bucket", bucket_name);
                                log.insert("object", object_key);
                                log.insert("region", aws_region);

                                for (key, value) in &metadata {
                                    log.insert(key, value.clone());
                                }

                                Some(Ok(event))
                            }
                            Err(err) => {
                                // TODO handling IO errors here?
                                dbg!(err);
                                None
                            }
                        }
                    }
                });

                out.send_all(Compat::new(Box::pin(stream)))
                    .compat()
                    .await
                    .map_err(|error| {
                        error!(message = "Error sending S3 Logs", %error);
                        ()
                    })
                    .map(|_| ())
            }
            None => Ok(()),
        }
    }

    async fn receive_messages(&self) -> Result<Vec<Message>, RusotoError<ReceiveMessageError>> {
        self.sqs_client
            .receive_message(ReceiveMessageRequest {
                queue_url: self.queue_url.clone(),
                max_number_of_messages: Some(10),
                // TODO handle timeouts > i64
                visibility_timeout: Some(self.visibility_timeout.as_secs().try_into().unwrap()),
                ..Default::default()
            })
            .map_ok(|res| res.messages.unwrap_or_default()) // TODO
            .await
    }

    async fn delete_message(
        &self,
        receipt_handle: Option<String>,
    ) -> Result<(), RusotoError<DeleteMessageError>> {
        let receipt_handle = receipt_handle.unwrap_or_default(); // TODO
        self.sqs_client
            .delete_message(DeleteMessageRequest {
                queue_url: self.queue_url.clone(),
                receipt_handle,
                ..Default::default()
            })
            .await
    }
}

// https://docs.aws.amazon.com/AmazonS3/latest/dev/notification-content-structure.html
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
struct S3Event {
    records: Vec<S3EventRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct S3EventRecord {
    event_version: String, // TODO compare >=
    event_source: String,
    aws_region: String, // TODO validate?
    event_name: String, // TODO break up?

    s3: S3Message,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct S3Message {
    bucket: S3Bucket,
    object: S3Object,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct S3Bucket {
    name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct S3Object {
    key: String,
}

#[cfg(feature = "aws-s3-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::test_util::{collect_n, random_lines};
    use pretty_assertions::assert_eq;
    use rusoto_s3::PutObjectRequest;

    #[tokio::test]
    async fn s3_process_message() {
        let s3 = s3_client();
        let sqs = sqs_client();

        let queue = create_queue(&sqs).await;
        let bucket = create_bucket(&s3, &queue).await;

        let config = config(&queue).await;

        let key = uuid::Uuid::new_v4().to_string();

        let logs: Vec<String> = random_lines(100).take(10).collect();

        s3.put_object(PutObjectRequest {
            bucket: bucket.clone(),
            key: key.clone(),
            body: Some(rusoto_core::ByteStream::from(logs.join("\n").into_bytes())),
            ..Default::default()
        })
        .await
        .expect("Could not put object");

        let (tx, rx) = Pipeline::new_test();
        tokio::spawn(async move {
            config
                .build(
                    "default",
                    &GlobalOptions::default(),
                    ShutdownSignal::noop(),
                    tx,
                )
                .await
                .unwrap()
                .compat()
                .await
                .unwrap()
        });

        let events = collect_n(rx, logs.len()).await.unwrap();

        assert_eq!(logs.len(), events.len());
        for (i, event) in events.iter().enumerate() {
            let message: String = logs[i].to_owned();

            let log = event.as_log();
            assert_eq!(log["message"], message.as_str().into());
            assert_eq!(log["bucket"], bucket.as_str().into());
            assert_eq!(log["object"], key.as_str().into());
            assert_eq!(log["region"], "us-east-1".into());
        }
    }

    async fn config(queue_name: &str) -> AwsS3Config {
        AwsS3Config {
            strategy: Strategy::Sqs,
            sqs: Some(SqsConfig {
                queue_name: queue_name.to_string(),
                region: Region::Custom {
                    name: "minio".to_owned(),
                    endpoint: "http://localhost:4566".to_owned(),
                },
                poll_secs: 1,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// creates a new SQS queue
    ///
    /// returns the queue name
    async fn create_queue(client: &SqsClient) -> String {
        use rusoto_sqs::CreateQueueRequest;

        let queue_name = uuid::Uuid::new_v4().to_string();

        client
            .create_queue(CreateQueueRequest {
                queue_name: queue_name.clone(),
                ..Default::default()
            })
            .await
            .expect("Could not create queue");

        queue_name
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
                notification_configuration: NotificationConfiguration {
                    queue_configurations: Some(vec![QueueConfiguration {
                        events: vec!["s3:ObjectCreated:*".to_string()],
                        queue_arn: format!("arn:aws:sqs:us-east-1:000000000000:{}", queue_name),
                        ..Default::default()
                    }]),
                    ..Default::default()
                },
                ..Default::default()
            })
            .await
            .expect("Could not create bucket notification");

        bucket_name
    }

    fn s3_client() -> S3Client {
        let region = Region::Custom {
            name: "minio".to_owned(),
            endpoint: "http://localhost:4566".to_owned(),
        };

        S3Client::new(region)
    }

    fn sqs_client() -> SqsClient {
        let region = Region::Custom {
            name: "minio".to_owned(),
            endpoint: "http://localhost:4566".to_owned(),
        };

        SqsClient::new(region)
    }
}
