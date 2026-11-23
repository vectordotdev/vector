use aws_sdk_firehose::types::{
    BufferingHints, DeliveryStreamStatus, ExtendedS3DestinationConfiguration,
};
use aws_sdk_s3::Client as S3Client;
use futures::StreamExt;
use serde_json::Value;
use tokio::time::{Duration, Instant, sleep};
use vector_lib::{codecs::JsonSerializerConfig, lookup::lookup_v2::ConfigValuePath};
use vrl::event_path;

use super::{config::KinesisFirehoseClientBuilder, *};
use crate::{
    aws::{AwsAuthentication, RegionOrEndpoint, create_client},
    config::{AcknowledgementsConfig, ProxyConfig, SinkConfig, SinkContext},
    event::Event,
    sinks::util::{BatchConfig, Compression, TowerRequestConfig},
    test_util::{
        components::{AWS_SINK_TAGS, run_and_assert_sink_compliance},
        random_events_with_stream, random_string, trace_init, wait_for_duration,
    },
};

/// The role that the emulator image creates for Firehose to write to S3.
const DELIVERY_ROLE_ARN: &str = "arn:aws:iam::000000000000:role/firehose-delivery";

fn kinesis_address() -> String {
    std::env::var("KINESIS_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
}

fn s3_address() -> String {
    std::env::var("S3_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
}

#[tokio::test]
async fn firehose_put_records_without_partition_key() {
    trace_init();
    let stream = gen_stream();

    ensure_s3_delivery_stream(&stream).await;

    let mut batch = BatchConfig::default();
    batch.max_events = Some(2);

    let config = KinesisFirehoseSinkConfig {
        base: base_config(&stream, None),
        batch,
    };

    let (sink, _) = config.build(SinkContext::default()).await.unwrap();

    let (input, events) = random_events_with_stream(100, 100, None);

    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;

    assert_delivered(&stream, input).await;
}

#[tokio::test]
async fn firehose_put_records_with_partition_key() {
    trace_init();
    let stream = gen_stream();

    ensure_s3_delivery_stream(&stream).await;

    let mut batch = BatchConfig::default();
    batch.max_events = Some(20);

    let partition_value = "a_value";
    let partition_key = ConfigValuePath::try_from("partition_key".to_string()).unwrap();

    let config = KinesisFirehoseSinkConfig {
        base: base_config(&stream, Some(partition_key)),
        batch,
    };

    let (sink, _) = config.build(SinkContext::default()).await.unwrap();

    let (mut input, events) = random_events_with_stream(100, 100, None);

    let events = events.map(move |mut events| {
        events.iter_logs_mut().for_each(move |log| {
            log.insert(event_path!("partition_key"), partition_value);
        });
        events
    });

    for log in &mut input {
        log.as_mut_log()
            .insert(event_path!("partition_key"), partition_value);
    }

    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;

    assert_delivered(&stream, input).await;
}

fn base_config(
    stream: &str,
    partition_key_field: Option<ConfigValuePath>,
) -> KinesisSinkBaseConfig {
    KinesisSinkBaseConfig {
        stream_name: stream.to_string(),
        region: test_region_endpoint(kinesis_address()),
        encoding: JsonSerializerConfig::default().into(),
        compression: Compression::None,
        request: TowerRequestConfig {
            timeout_secs: 10,
            retry_attempts: 0,
            ..Default::default()
        },
        tls: None,
        auth: AwsAuthentication::default(),
        acknowledgements: AcknowledgementsConfig::default(),
        request_retry_partial: false,
        partition_key_field,
    }
}

/// Waits until Firehose has written every input event to the stream's bucket, then checks that
/// the delivered records are exactly the input events.
async fn assert_delivered(bucket: &str, input: Vec<Event>) {
    let mut expected = input
        .into_iter()
        .map(|event| serde_json::to_value(event.into_log()).unwrap().to_string())
        .collect::<Vec<_>>();
    expected.sort();

    let client = s3_client().await;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let mut delivered = delivered_records(&client, bucket).await;
        if delivered.len() >= expected.len() || Instant::now() >= deadline {
            delivered.sort();
            assert_eq!(expected, delivered);
            return;
        }
        sleep(Duration::from_millis(500)).await;
    }
}

/// Reads every record that Firehose delivered to `bucket`. Firehose concatenates the records of
/// a delivery into one object, so each object holds a sequence of JSON values.
async fn delivered_records(client: &S3Client, bucket: &str) -> Vec<String> {
    let objects = client
        .list_objects_v2()
        .bucket(bucket)
        .send()
        .await
        .expect("could not list delivered objects");

    let mut records = Vec::new();
    for object in objects.contents() {
        let body = client
            .get_object()
            .bucket(bucket)
            .key(object.key().expect("delivered object has no key"))
            .send()
            .await
            .expect("could not get delivered object")
            .body
            .collect()
            .await
            .expect("could not read delivered object")
            .into_bytes();
        records.extend(
            serde_json::Deserializer::from_slice(&body)
                .into_iter::<Value>()
                .map(|record| record.expect("delivered record is not JSON").to_string()),
        );
    }
    records
}

fn test_region_endpoint(endpoint: String) -> RegionOrEndpoint {
    RegionOrEndpoint::with_both("us-east-1", endpoint)
}

async fn firehose_client() -> aws_sdk_firehose::Client {
    let region_endpoint = test_region_endpoint(kinesis_address());

    create_client::<KinesisFirehoseClientBuilder>(
        &KinesisFirehoseClientBuilder {},
        &AwsAuthentication::test_auth(),
        region_endpoint.region(),
        region_endpoint.endpoint(),
        &ProxyConfig::default(),
        None,
        None,
    )
    .await
    .unwrap()
}

async fn s3_client() -> S3Client {
    struct S3ClientBuilder;
    impl crate::aws::ClientBuilder for S3ClientBuilder {
        type Client = S3Client;

        fn build(&self, config: &aws_types::SdkConfig) -> Self::Client {
            let config = aws_sdk_s3::config::Builder::from(config)
                .force_path_style(true)
                .build();
            S3Client::from_conf(config)
        }
    }

    let region_endpoint = test_region_endpoint(s3_address());
    create_client::<S3ClientBuilder>(
        &S3ClientBuilder,
        &AwsAuthentication::test_auth(),
        region_endpoint.region(),
        region_endpoint.endpoint(),
        &ProxyConfig::default(),
        None,
        None,
    )
    .await
    .unwrap()
}

/// Creates a bucket and a Firehose delivery stream, both named `name`, that writes each record to
/// the bucket without buffering. Returns when the delivery stream accepts records.
async fn ensure_s3_delivery_stream(name: &str) {
    s3_client()
        .await
        .create_bucket()
        .bucket(name)
        .send()
        .await
        .expect("could not create the destination bucket");

    let client = firehose_client().await;
    let request = client
        .create_delivery_stream()
        .delivery_stream_name(name)
        .extended_s3_destination_configuration(
            ExtendedS3DestinationConfiguration::builder()
                .role_arn(DELIVERY_ROLE_ARN)
                .bucket_arn(format!("arn:aws:s3:::{name}"))
                .buffering_hints(
                    BufferingHints::builder()
                        .interval_in_seconds(0)
                        .size_in_mbs(1)
                        .build(),
                )
                .build()
                .expect("all required builder fields populated"),
        )
        .send();
    Box::pin(request)
        .await
        .expect("could not create the delivery stream");

    let name = name.to_string();
    wait_for_duration(
        move || {
            let client = client.clone();
            let name = name.clone();
            async move {
                client
                    .describe_delivery_stream()
                    .delivery_stream_name(name)
                    .send()
                    .await
                    .is_ok_and(|output| {
                        output
                            .delivery_stream_description()
                            .is_some_and(|description| {
                                *description.delivery_stream_status()
                                    == DeliveryStreamStatus::Active
                            })
                    })
            }
        },
        Duration::from_secs(30),
    )
    .await;
}

fn gen_stream() -> String {
    format!("test-{}", random_string(10).to_lowercase())
}
