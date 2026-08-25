use std::{
    any::Any,
    collections::HashMap,
    fs::File,
    io::{self, BufRead},
    path::Path,
    time::Duration,
};

use aws_sdk_s3::Client as S3Client;
use aws_sdk_sqs::{Client as SqsClient, types::QueueAttributeName};
use similar_asserts::assert_eq;
use vector_lib::{
    codecs::{JsonDeserializerConfig, decoding::DeserializerConfig},
    lookup::path,
};
use vrl::value::Value;

use super::*;
use crate::{
    SourceSender,
    aws::{AwsAuthentication, RegionOrEndpoint, create_client},
    common::sqs::SqsClientBuilder,
    config::{ProxyConfig, SourceConfig, SourceContext},
    event::EventStatus::{self, *},
    line_agg,
    sources::{
        aws_s3::{S3ClientBuilder, sqs::S3Event},
        util::MultilineConfig,
    },
    test_util::{
        collect_n,
        components::{SOURCE_TAGS, assert_source_compliance},
        lines_from_gzip_file, random_lines, trace_init,
    },
};

fn lines_from_plaintext<P: AsRef<Path>>(path: P) -> Vec<String> {
    let file = io::BufReader::new(File::open(path).unwrap());
    file.lines().map(|x| x.unwrap()).collect()
}

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
        false,
        DeserializerConfig::Bytes,
        None,
    )
    .await;
}

#[tokio::test]
async fn s3_process_json_message() {
    trace_init();

    let logs: Vec<String> = random_lines(100).take(10).collect();

    let json_logs: Vec<String> = logs
        .iter()
        .map(|msg| {
            // convert to JSON object
            format!(r#"{{"message": "{msg}"}}"#)
        })
        .collect();

    test_event(
        None,
        None,
        None,
        None,
        json_logs.join("\n").into_bytes(),
        logs,
        Delivered,
        false,
        DeserializerConfig::Json(JsonDeserializerConfig::default()),
        None,
    )
    .await;
}

#[tokio::test]
async fn s3_process_message_with_log_namespace() {
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
        true,
        DeserializerConfig::Bytes,
        None,
    )
    .await;
}

#[tokio::test]
async fn s3_process_message_spaces() {
    trace_init();

    let key = "key with spaces".to_string();
    let logs: Vec<String> = random_lines(100).take(10).collect();

    test_event(
        Some(key),
        None,
        None,
        None,
        logs.join("\n").into_bytes(),
        logs,
        Delivered,
        false,
        DeserializerConfig::Bytes,
        None,
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
        false,
        DeserializerConfig::Bytes,
        None,
    )
    .await;
}

#[tokio::test]
async fn s3_process_message_gzip() {
    use std::io::Read;

    trace_init();

    let logs: Vec<String> = random_lines(100).take(10).collect();

    let mut gz = flate2::read::GzEncoder::new(
        io::Cursor::new(logs.join("\n").into_bytes()),
        flate2::Compression::fast(),
    );
    let mut buffer = Vec::new();
    gz.read_to_end(&mut buffer).unwrap();

    test_event(
        None,
        Some("gzip"),
        None,
        None,
        buffer,
        logs,
        Delivered,
        false,
        DeserializerConfig::Bytes,
        None,
    )
    .await;
}

#[tokio::test]
async fn s3_process_message_multipart_gzip() {
    use std::io::Read;

    trace_init();

    let logs = lines_from_gzip_file("tests/data/multipart-gzip.log.gz");

    let buffer = {
        let mut file = File::open("tests/data/multipart-gzip.log.gz").expect("file can be opened");
        let mut data = Vec::new();
        file.read_to_end(&mut data).expect("file can be read");
        data
    };

    test_event(
        None,
        Some("gzip"),
        None,
        None,
        buffer,
        logs,
        Delivered,
        false,
        DeserializerConfig::Bytes,
        None,
    )
    .await;
}

#[tokio::test]
async fn s3_process_message_multipart_zstd() {
    use std::io::Read;

    trace_init();

    let logs = lines_from_plaintext("tests/data/multipart-zst.log");

    let buffer = {
        let mut file = File::open("tests/data/multipart-zst.log.zst").expect("file can be opened");
        let mut data = Vec::new();
        file.read_to_end(&mut data).expect("file can be read");
        data
    };

    test_event(
        None,
        Some("zstd"),
        None,
        None,
        buffer,
        logs,
        Delivered,
        false,
        DeserializerConfig::Bytes,
        None,
    )
    .await;
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
            timeout_ms: Duration::from_millis(1000),
        }),
        logs.join("\n").into_bytes(),
        vec!["abc\ndef\ngeh".to_owned()],
        Delivered,
        false,
        DeserializerConfig::Bytes,
        None,
    )
    .await;
}

// TODO: re-enable this after figuring out why it is so flakey in CI
//       https://github.com/vectordotdev/vector/issues/17456
#[ignore]
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
        false,
        DeserializerConfig::Bytes,
        None,
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
        false,
        DeserializerConfig::Bytes,
        None,
    )
    .await;
}

#[tokio::test]
async fn handles_failed_status_without_deletion() {
    trace_init();

    let logs: Vec<String> = random_lines(100).take(10).collect();

    let mut custom_options: HashMap<String, Box<dyn Any>> = HashMap::new();
    custom_options.insert("delete_failed_message".to_string(), Box::new(false));

    test_event(
        None,
        None,
        None,
        None,
        logs.join("\n").into_bytes(),
        logs,
        Rejected,
        false,
        DeserializerConfig::Bytes,
        Some(custom_options),
    )
    .await;
}

fn s3_address() -> String {
    std::env::var("S3_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
}

fn config(
    queue_url: &str,
    multiline: Option<MultilineConfig>,
    log_namespace: bool,
    decoding: DeserializerConfig,
) -> AwsS3Config {
    AwsS3Config {
        region: RegionOrEndpoint::with_both("us-east-1", s3_address()),
        strategy: Strategy::Sqs,
        compression: Compression::Auto,
        multiline,
        sqs: Some(sqs::Config {
            queue_url: queue_url.to_string(),
            poll_secs: 1,
            max_number_of_messages: 10,
            visibility_timeout_secs: 0,
            client_concurrency: None,
            ..Default::default()
        }),
        acknowledgements: true.into(),
        log_namespace: Some(log_namespace),
        decoding,
        ..Default::default()
    }
}

// puts an object and asserts that the logs it gets back match
#[allow(clippy::too_many_arguments)]
async fn test_event(
    key: Option<String>,
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    multiline: Option<MultilineConfig>,
    payload: Vec<u8>,
    expected_lines: Vec<String>,
    status: EventStatus,
    log_namespace: bool,
    decoding: DeserializerConfig,
    custom_options: Option<HashMap<String, Box<dyn Any>>>,
) {
    assert_source_compliance(&SOURCE_TAGS, async move {
        let key = key.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let s3 = s3_client().await;
        let sqs = sqs_client().await;

        let queue = create_queue(&sqs).await;
        let bucket = create_bucket(&s3).await;

        tokio::time::sleep(Duration::from_secs(1)).await;

        let mut config = config(&queue, multiline, log_namespace, decoding);

        if let Some(false) = custom_options
            .as_ref()
            .and_then(|opts| opts.get("delete_failed_message"))
            .and_then(|val| val.downcast_ref::<bool>())
            .copied()
        {
            config.sqs.as_mut().unwrap().delete_failed_message = false;
        }

        s3.put_object()
            .bucket(bucket.clone())
            .key(key.clone())
            .body(ByteStream::from(payload))
            .set_content_type(content_type.map(|t| t.to_owned()))
            .set_content_encoding(content_encoding.map(|t| t.to_owned()))
            .send()
            .await
            .expect("Could not put object");

        let sqs_client = sqs_client().await;

        let mut s3_event: S3Event = serde_json::from_str(
        r#"
{
   "Records":[
  {
     "eventVersion":"2.1",
     "eventSource":"aws:s3",
     "awsRegion":"us-east-1",
     "eventTime":"2022-03-24T19:43:00.548Z",
     "eventName":"ObjectCreated:Put",
     "userIdentity":{
        "principalId":"AWS:ARNOTAREALIDD4:user.name"
     },
     "requestParameters":{
        "sourceIPAddress":"136.56.73.213"
     },
     "responseElements":{
        "x-amz-request-id":"ZX6X98Q6NM9NQTP3",
        "x-amz-id-2":"ESLLtyT4N5cAPW+C9EXwtaeEWz6nq7eCA6txjZKlG2Q7xp2nHXQI69Od2B0PiYIbhUiX26NrpIQPV0lLI6js3nVNmYo2SWBs"
     },
     "s3":{
        "s3SchemaVersion":"1.0",
        "configurationId":"asdfasdf",
        "bucket":{
           "name":"bucket-name",
           "ownerIdentity":{
              "principalId":"A3PEG170DF9VNQ"
           },
           "arn":"arn:aws:s3:::nfox-testing-vector"
        },
        "object":{
           "key":"test-log.txt",
           "size":33,
           "eTag":"c981ce6672c4251048b0b834e334007f",
           "sequencer":"00623CC9C47AB5634C"
        }
     }
  }
   ]
}
    "#,
        )
        .unwrap();

        s3_event.records[0].s3.bucket.name.clone_from(&bucket);
        s3_event.records[0].s3.object.key.clone_from(&key);

        // send SQS message (this is usually sent by S3 itself when an object is uploaded)
        // This does not automatically work with localstack and the AWS SDK, so this is done manually
        let _send_message_output = sqs_client
            .send_message()
            .queue_url(queue.clone())
            .message_body(serde_json::to_string(&s3_event).unwrap())
            .send()
            .await
            .unwrap();

        let (tx, rx) = SourceSender::new_test_finalize(status);
        let cx = SourceContext::new_test(tx, None);
        let namespace = cx.log_namespace(Some(log_namespace));
        let source = config.build(cx).await.unwrap();
        tokio::spawn(async move { source.await.unwrap() });

        let events = collect_n(rx, expected_lines.len()).await;

        assert_eq!(expected_lines.len(), events.len());
        for (i, event) in events.iter().enumerate() {

            if let Some(schema_definition) = config.outputs(namespace).pop().unwrap().schema_definition {
                schema_definition.is_valid_for_event(event).unwrap();
            }

            let message = expected_lines[i].as_str();

            let log = event.as_log();
            if log_namespace {
                assert_eq!(log.value(), &Value::from(message));
            } else {
                assert_eq!(log["message"], message.into());
            }
            assert_eq!(namespace.get_source_metadata(AwsS3Config::NAME, log, path!("bucket"), path!("bucket")).unwrap(), &bucket.clone().into());
            assert_eq!(namespace.get_source_metadata(AwsS3Config::NAME, log, path!("object"), path!("object")).unwrap(), &key.clone().into());
            assert_eq!(namespace.get_source_metadata(AwsS3Config::NAME, log, path!("region"), path!("region")).unwrap(), &"us-east-1".into());
        }

        // Unfortunately we need a fairly large sleep here to ensure that the source has actually managed to delete the SQS message.
        // The deletion of this message occurs after the Event has been sent out by the source and there is no way of knowing when this
        // process has finished other than waiting around for a while.
        tokio::time::sleep(Duration::from_secs(10)).await;
        // Make sure the SQS message is deleted
        match status {
            Errored => {
                // need to wait up to the visibility timeout before it will be counted again
                assert_eq!(count_messages(&sqs, &queue, 10).await, 1);
            }
            Rejected if !config.sqs.unwrap().delete_failed_message => {
                assert_eq!(count_messages(&sqs, &queue, 10).await, 1);
            }
            _ => {
                assert_eq!(count_messages(&sqs, &queue, 0).await, 0);
            }
        };
    }).await;
}

/// creates a new SQS queue
///
/// returns the queue name
async fn create_queue(client: &SqsClient) -> String {
    let queue_name = uuid::Uuid::new_v4().to_string();

    let res = client
        .create_queue()
        .queue_name(queue_name.clone())
        .attributes(QueueAttributeName::VisibilityTimeout, "2")
        .send()
        .await
        .expect("Could not create queue");

    res.queue_url.expect("no queue url")
}

/// count the number of messages in a SQS queue
async fn count_messages(client: &SqsClient, queue: &str, wait_time_seconds: i32) -> usize {
    let sqs_result = client
        .receive_message()
        .queue_url(queue)
        .visibility_timeout(0)
        .wait_time_seconds(wait_time_seconds)
        .send()
        .await
        .unwrap();

    sqs_result
        .messages
        .map(|messages| messages.len())
        .unwrap_or(0)
}

/// creates a new S3 bucket
///
/// returns the bucket name
async fn create_bucket(client: &S3Client) -> String {
    let bucket_name = uuid::Uuid::new_v4().to_string();

    client
        .create_bucket()
        .bucket(bucket_name.clone())
        .send()
        .await
        .expect("Could not create bucket");

    bucket_name
}

async fn s3_client() -> S3Client {
    let auth = AwsAuthentication::test_auth();
    let region_endpoint = RegionOrEndpoint {
        region: Some("us-east-1".to_owned()),
        endpoint: Some(s3_address()),
    };
    let proxy_config = ProxyConfig::default();
    let force_path_style_value: bool = true;
    create_client::<S3ClientBuilder>(
        &S3ClientBuilder {
            force_path_style: Some(force_path_style_value),
        },
        &auth,
        region_endpoint.region(),
        region_endpoint.endpoint(),
        &proxy_config,
        None,
        None,
    )
    .await
    .unwrap()
}

async fn sqs_client() -> SqsClient {
    let auth = AwsAuthentication::test_auth();
    let region_endpoint = RegionOrEndpoint {
        region: Some("us-east-1".to_owned()),
        endpoint: Some(s3_address()),
    };
    let proxy_config = ProxyConfig::default();
    create_client::<SqsClientBuilder>(
        &SqsClientBuilder {},
        &auth,
        region_endpoint.region(),
        region_endpoint.endpoint(),
        &proxy_config,
        None,
        None,
    )
    .await
    .unwrap()
}
