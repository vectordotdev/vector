use aws_sdk_firehose::types::ExtendedS3DestinationConfiguration;
use aws_sdk_s3::Client as S3Client;
use futures::StreamExt;
use vector_lib::{codecs::JsonSerializerConfig, lookup::lookup_v2::ConfigValuePath};

use super::{config::KinesisFirehoseClientBuilder, *};
use crate::{
    aws::{AwsAuthentication, RegionOrEndpoint, create_client},
    common::s3::S3ClientBuilder,
    config::{ProxyConfig, SinkConfig, SinkContext},
    sinks::util::{BatchConfig, Compression, TowerRequestConfig},
    test_util::{
        components::{AWS_SINK_TAGS, run_and_assert_sink_compliance},
        random_events_with_stream, random_string,
    },
};

fn kinesis_address() -> String {
    std::env::var("KINESIS_ADDRESS").unwrap_or_else(|_| "http://localhost:5000".into())
}

fn s3_address() -> String {
    std::env::var("S3_ADDRESS").unwrap_or_else(|_| "http://localhost:5000".into())
}

#[tokio::test]
async fn firehose_put_records_without_partition_key() {
    let stream = gen_stream();
    let bucket = gen_stream();

    ensure_s3_bucket(&bucket).await;
    ensure_s3_delivery_stream(&stream, &bucket).await;

    let mut batch = BatchConfig::default();
    batch.max_events = Some(2);

    let region = RegionOrEndpoint::with_both("us-east-1", kinesis_address().as_str());

    let base = KinesisSinkBaseConfig {
        stream_name: stream.clone(),
        region: region.clone(),
        encoding: JsonSerializerConfig::default().into(),
        compression: Compression::None,
        request: TowerRequestConfig {
            timeout_secs: 10,
            retry_attempts: 0,
            ..Default::default()
        },
        tls: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
        request_retry_partial: Default::default(),
        partition_key_field: None,
    };

    let config = KinesisFirehoseSinkConfig { batch, base };

    let cx = SinkContext::default();

    let (sink, _) = config.build(cx).await.unwrap();

    let (input, events) = random_events_with_stream(100, 100, None);

    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;

    let records = read_records_from_s3(&bucket).await;

    assert_eq!(input.len() as u64, records.len() as u64);

    let input = input
        .into_iter()
        .map(|rec| serde_json::to_value(rec.into_log()).unwrap())
        .collect::<Vec<_>>();
    for record in &records {
        assert!(input.contains(record));
    }
}

#[tokio::test]
async fn firehose_put_records_with_partition_key() {
    let stream = gen_stream();
    let bucket = gen_stream();

    ensure_s3_bucket(&bucket).await;
    ensure_s3_delivery_stream(&stream, &bucket).await;

    let mut batch = BatchConfig::default();
    batch.max_events = Some(20);

    let region = RegionOrEndpoint::with_both("us-east-1", kinesis_address().as_str());

    let partition_value = "a_value";
    let partition_key = ConfigValuePath::try_from("partition_key".to_string()).unwrap();

    let base = KinesisSinkBaseConfig {
        stream_name: stream.clone(),
        region: region.clone(),
        encoding: JsonSerializerConfig::default().into(),
        compression: Compression::None,
        request: TowerRequestConfig {
            timeout_secs: 10,
            retry_attempts: 0,
            ..Default::default()
        },
        tls: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
        request_retry_partial: Default::default(),
        partition_key_field: Some(partition_key.clone()),
    };

    let config = KinesisFirehoseSinkConfig { batch, base };

    let cx = SinkContext::default();

    let (sink, _) = config.build(cx).await.unwrap();

    let (mut input, events) = random_events_with_stream(100, 100, None);

    let events = events.map(move |mut events| {
        events.iter_logs_mut().for_each(move |log| {
            log.insert("partition_key", partition_value);
        });
        events
    });

    input.iter_mut().for_each(move |log| {
        log.as_mut_log().insert("partition_key", partition_value);
    });

    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;

    let records = read_records_from_s3(&bucket).await;

    assert_eq!(input.len() as u64, records.len() as u64);

    let input = input
        .into_iter()
        .map(|rec| serde_json::to_value(rec.into_log()).unwrap())
        .collect::<Vec<_>>();
    for record in &records {
        assert!(input.contains(record));
    }
}

fn test_region_endpoint() -> RegionOrEndpoint {
    RegionOrEndpoint::with_both("us-east-1", kinesis_address())
}

async fn firehose_client() -> aws_sdk_firehose::Client {
    let region_endpoint = test_region_endpoint();
    let auth = AwsAuthentication::test_auth();
    let proxy = ProxyConfig::default();

    create_client::<KinesisFirehoseClientBuilder>(
        &KinesisFirehoseClientBuilder {},
        &auth,
        region_endpoint.region(),
        region_endpoint.endpoint(),
        &proxy,
        None,
        None,
    )
    .await
    .unwrap()
}

async fn s3_client() -> S3Client {
    let region = RegionOrEndpoint::with_both("us-east-1", s3_address());
    let auth = AwsAuthentication::test_auth();
    let proxy = ProxyConfig::default();

    create_client::<S3ClientBuilder>(
        &S3ClientBuilder {
            force_path_style: Some(true),
        },
        &auth,
        region.region(),
        region.endpoint(),
        &proxy,
        None,
        None,
    )
    .await
    .unwrap()
}

async fn ensure_s3_bucket(bucket: &str) {
    s3_client()
        .await
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("Failed to create S3 bucket");
}

async fn ensure_s3_delivery_stream(delivery_stream_name: &str, bucket_name: &str) {
    let client = firehose_client().await;
    let bucket_arn = format!("arn:aws:s3:::{}", bucket_name);

    client
        .create_delivery_stream()
        .delivery_stream_name(delivery_stream_name)
        .extended_s3_destination_configuration(
            ExtendedS3DestinationConfiguration::builder()
                .bucket_arn(bucket_arn)
                .role_arn("arn:aws:iam::123456789012:role/firehose-role")
                .build()
                .expect("all builder fields populated"),
        )
        .send()
        .await
        .expect("Failed to create Firehose delivery stream");
}

/// Read all records delivered by Firehose into the S3 bucket. Moto writes each
/// put_record_batch call as a single S3 object containing the raw concatenated
/// bytes of all records in that batch, so we stream-parse each object as a
/// sequence of JSON values.
async fn read_records_from_s3(bucket: &str) -> Vec<serde_json::Value> {
    let client = s3_client().await;

    let objects = client
        .list_objects_v2()
        .bucket(bucket)
        .send()
        .await
        .expect("Failed to list S3 objects");

    let mut records = Vec::new();
    for obj in objects.contents() {
        let key = obj.key().expect("S3 object missing key");
        let output = client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .expect("Failed to get S3 object");
        let body = output
            .body
            .collect()
            .await
            .expect("Failed to read S3 object body")
            .into_bytes();
        let mut de =
            serde_json::Deserializer::from_slice(&body).into_iter::<serde_json::Value>();
        while let Some(Ok(value)) = de.next() {
            records.push(value);
        }
    }
    records
}

fn gen_stream() -> String {
    format!("test-{}", random_string(10).to_lowercase())
}
