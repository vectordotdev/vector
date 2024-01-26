#![cfg(all(test, feature = "aws-s3-integration-tests"))]

use std::{
    io::{BufRead, BufReader},
    time::Duration,
};

use aws_sdk_s3::{
    operation::{create_bucket::CreateBucketError, get_object::GetObjectOutput},
    types::{
        DefaultRetention, ObjectLockConfiguration, ObjectLockEnabled, ObjectLockRetentionMode,
        ObjectLockRule,
    },
    Client as S3Client,
};
use aws_smithy_runtime_api::client::result::SdkError;
use bytes::Buf;
use flate2::read::MultiGzDecoder;
use futures::{stream, Stream};
use similar_asserts::assert_eq;
use tokio_stream::StreamExt;
use vector_lib::codecs::{encoding::FramingConfig, TextSerializerConfig};
use vector_lib::{
    config::proxy::ProxyConfig,
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, EventArray, LogEvent},
};

use super::S3SinkConfig;
use crate::{
    aws::{create_client, AwsAuthentication, RegionOrEndpoint},
    common::s3::S3ClientBuilder,
    config::SinkContext,
    sinks::{
        aws_s3::config::default_filename_time_format,
        s3_common::config::{S3Options, S3ServerSideEncryption},
        util::{BatchConfig, Compression, TowerRequestConfig},
    },
    test_util::{
        components::{
            run_and_assert_sink_compliance, run_and_assert_sink_error, AWS_SINK_TAGS,
            COMPONENT_ERROR_TAGS,
        },
        random_lines_with_stream, random_string,
    },
};

fn s3_address() -> String {
    std::env::var("S3_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
}

#[tokio::test]
async fn s3_insert_message_into_with_flat_key_prefix() {
    let cx = SinkContext::default();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let mut config = config(&bucket, 1000000);
    config.key_prefix = "test-prefix".to_string();
    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service, cx).unwrap();

    let (lines, events, receiver) = make_events_batch(100, 10);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = get_keys(&bucket, prefix).await;
    assert_eq!(keys.len(), 1);

    let key = keys[0].clone();
    let key_parts = key.split('/');
    assert!(key_parts.count() == 1);
    assert!(key.starts_with("test-prefix"));
    assert!(key.ends_with(".log"));

    let obj = get_object(&bucket, key).await;
    assert_eq!(obj.content_encoding, None);

    let response_lines = get_lines(obj).await;
    assert_eq!(lines, response_lines);
}

#[tokio::test]
async fn s3_insert_message_into_with_folder_key_prefix() {
    let cx = SinkContext::default();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let mut config = config(&bucket, 1000000);
    config.key_prefix = "test-prefix/".to_string();
    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service, cx).unwrap();

    let (lines, events, receiver) = make_events_batch(100, 10);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = get_keys(&bucket, prefix).await;
    assert_eq!(keys.len(), 1);

    let key = keys[0].clone();
    let key_parts = key.split('/').collect::<Vec<_>>();
    assert!(key_parts.len() == 2);
    assert!(*key_parts.first().unwrap() == "test-prefix");
    assert!(key.ends_with(".log"));

    let obj = get_object(&bucket, key).await;
    assert_eq!(obj.content_encoding, None);

    let response_lines = get_lines(obj).await;
    assert_eq!(lines, response_lines);
}

#[tokio::test]
async fn s3_insert_message_into_with_ssekms_key_id() {
    let cx = SinkContext::default();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let mut config = config(&bucket, 1000000);
    config.key_prefix = "test-prefix".to_string();
    let prefix = config.key_prefix.clone();
    config.options.server_side_encryption = Some(S3ServerSideEncryption::AwsKms);
    config.options.ssekms_key_id = Some("alias/aws/s3".to_string());

    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service, cx).unwrap();

    let (lines, events, receiver) = make_events_batch(100, 10);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = get_keys(&bucket, prefix).await;
    assert_eq!(keys.len(), 1);

    let key = keys[0].clone();
    let key_parts = key.split('/');
    assert!(key_parts.count() == 1);
    assert!(key.starts_with("test-prefix"));
    assert!(key.ends_with(".log"));

    let obj = get_object(&bucket, key).await;
    assert_eq!(obj.content_encoding, None);

    let response_lines = get_lines(obj).await;
    assert_eq!(lines, response_lines);
}

#[tokio::test]
async fn s3_rotate_files_after_the_buffer_size_is_reached() {
    let cx = SinkContext::default();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let config = S3SinkConfig {
        key_prefix: format!("{}/{}", random_string(10), "{{i}}"),
        filename_time_format: "waitsforfullbatch".into(),
        filename_append_uuid: false,
        ..config(&bucket, 10)
    };
    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service, cx).unwrap();

    let (lines, _events) = random_lines_with_stream(100, 30, None);

    let events = lines.clone().into_iter().enumerate().map(|(i, line)| {
        let mut e = LogEvent::from(line);
        let i = if i < 10 {
            1
        } else if i < 20 {
            2
        } else {
            3
        };
        e.insert("i", i.to_string());
        Event::from(e)
    });

    run_and_assert_sink_compliance(sink, stream::iter(events), &AWS_SINK_TAGS).await;

    // Hard-coded sleeps are bad, but we're waiting on localstack's state to converge.
    tokio::time::sleep(Duration::from_secs(1)).await;

    let keys = get_keys(&bucket, prefix).await;
    assert_eq!(keys.len(), 3);

    let mut response_lines: Vec<Vec<String>> = Vec::new();
    let mut key_stream = stream::iter(keys);
    while let Some(key) = key_stream.next().await {
        let lines: Vec<String> = get_lines(get_object(&bucket, key).await).await;
        response_lines.push(lines);
    }

    assert_eq!(&lines[00..10], response_lines[0].as_slice());
    assert_eq!(&lines[10..20], response_lines[1].as_slice());
    assert_eq!(&lines[20..30], response_lines[2].as_slice());
}

#[tokio::test]
async fn s3_gzip() {
    // Here, we're creating a bunch of events, approximately 3000, while setting our batch size
    // to 1000, and using gzip compression.  We test to ensure that all of the keys we end up
    // writing represent the sum total of the lines: we expect 3 batches, each of which should
    // have 1000 lines.
    let cx = SinkContext::default();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let batch_size = 1_000;
    let batch_multiplier = 3;
    let config = S3SinkConfig {
        compression: Compression::gzip_default(),
        filename_time_format: "%s%f".into(),
        ..config(&bucket, batch_size)
    };

    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service, cx).unwrap();

    let (lines, events, receiver) = make_events_batch(100, batch_size * batch_multiplier);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = get_keys(&bucket, prefix).await;
    assert_eq!(keys.len(), batch_multiplier);

    let mut response_lines: Vec<String> = Vec::new();
    let mut key_stream = stream::iter(keys);
    while let Some(key) = key_stream.next().await {
        assert!(key.ends_with(".log.gz"));

        let obj = get_object(&bucket, key).await;
        assert_eq!(obj.content_encoding, Some("gzip".to_string()));

        response_lines.append(&mut get_gzipped_lines(obj).await);
    }

    assert_eq!(lines, response_lines);
}

#[tokio::test]
async fn s3_zstd() {
    // Here, we're creating a bunch of events, approximately 3000, while setting our batch size
    // to 1000, and using zstd compression.  We test to ensure that all of the keys we end up
    // writing represent the sum total of the lines: we expect 3 batches, each of which should
    // have 1000 lines.
    let cx = SinkContext::default();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let batch_size = 1_000;
    let batch_multiplier = 3;
    let config = S3SinkConfig {
        compression: Compression::zstd_default(),
        filename_time_format: "%s%f".into(),
        ..config(&bucket, batch_size)
    };

    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service, cx).unwrap();

    let (lines, events, receiver) = make_events_batch(100, batch_size * batch_multiplier);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = get_keys(&bucket, prefix).await;
    assert_eq!(keys.len(), batch_multiplier);

    let mut response_lines: Vec<String> = Vec::new();
    let mut key_stream = stream::iter(keys);
    while let Some(key) = key_stream.next().await {
        assert!(key.ends_with(".log.zst"));

        let obj = get_object(&bucket, key).await;
        assert_eq!(obj.content_encoding, Some("zstd".to_string()));

        response_lines.append(&mut get_zstd_lines(obj).await);
    }

    assert_eq!(lines, response_lines);
}

// NOTE: this test doesn't actually validate anything because localstack
// doesn't enforce the required Content-MD5 header on the request for
// buckets with object lock enabled
// https://github.com/localstack/localstack/issues/4166
#[tokio::test]
async fn s3_insert_message_into_object_lock() {
    let cx = SinkContext::default();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, true).await;

    client()
        .await
        .put_object_lock_configuration()
        .bucket(bucket.to_string())
        .object_lock_configuration(
            ObjectLockConfiguration::builder()
                .object_lock_enabled(ObjectLockEnabled::Enabled)
                .rule(
                    ObjectLockRule::builder()
                        .default_retention(
                            DefaultRetention::builder()
                                .days(1)
                                .mode(ObjectLockRetentionMode::Governance)
                                .set_years(None)
                                .build(),
                        )
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .unwrap();

    let config = config(&bucket, 1000000);
    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service, cx).unwrap();

    let (lines, events, receiver) = make_events_batch(100, 10);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = get_keys(&bucket, prefix).await;
    assert_eq!(keys.len(), 1);

    let key = keys[0].clone();
    assert!(key.ends_with(".log"));

    let obj = get_object(&bucket, key).await;
    assert_eq!(obj.content_encoding, None);

    let response_lines = get_lines(obj).await;
    assert_eq!(lines, response_lines);
}

#[tokio::test]
async fn acknowledges_failures() {
    let cx = SinkContext::default();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let mut config = config(&bucket, 1);
    // Break the bucket name
    config.bucket = format!("BREAK{}IT", config.bucket);
    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service, cx).unwrap();

    let (_lines, events, receiver) = make_events_batch(1, 1);
    run_and_assert_sink_error(sink, events, &COMPONENT_ERROR_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Rejected);

    let objects = list_objects(&bucket, prefix).await;
    assert_eq!(objects, None);
}

#[tokio::test]
async fn s3_healthchecks() {
    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let config = config(&bucket, 1);
    let service = config
        .create_service(&ProxyConfig::from_env())
        .await
        .unwrap();
    config.build_healthcheck(service.client()).unwrap();
}

#[tokio::test]
async fn s3_healthchecks_invalid_bucket() {
    let config = config("s3_healthchecks_invalid_bucket", 1);
    let service = config
        .create_service(&ProxyConfig::from_env())
        .await
        .unwrap();
    assert!(config
        .build_healthcheck(service.client())
        .unwrap()
        .await
        .is_err());
}

#[tokio::test]
async fn s3_flush_on_exhaustion() {
    let cx = SinkContext::default();

    let bucket = uuid::Uuid::new_v4().to_string();
    create_bucket(&bucket, false).await;

    // batch size of ten events, timeout of ten seconds
    let config = {
        let mut batch = BatchConfig::default();
        batch.max_events = Some(10);
        batch.timeout_secs = Some(10.0);

        S3SinkConfig {
            bucket: bucket.to_string(),
            key_prefix: random_string(10) + "/date=%F",
            filename_time_format: default_filename_time_format(),
            filename_append_uuid: true,
            filename_extension: None,
            options: S3Options::default(),
            region: RegionOrEndpoint::with_both("us-east-1", s3_address()),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Compression::None,
            batch,
            request: TowerRequestConfig::default(),
            tls: Default::default(),
            auth: Default::default(),
            acknowledgements: Default::default(),
            timezone: Default::default(),
        }
    };
    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service, cx).unwrap();

    let (lines, _events) = random_lines_with_stream(100, 2, None); // only generate two events (less than batch size)

    let events = lines.clone().into_iter().enumerate().map(|(i, line)| {
        let mut e = LogEvent::from(line);
        let i = if i < 10 {
            1
        } else if i < 20 {
            2
        } else {
            3
        };
        e.insert("i", i.to_string());
        Event::from(e)
    });

    // Here, we validate that the s3 sink flushes when its source stream is exhausted
    // by giving it a number of inputs less than the batch size, verifying that the
    // outputs for the in-flight batch are flushed. By timing out in 3 seconds with a
    // flush period of ten seconds, we verify that the flush is triggered *at stream
    // completion* and not because of periodic flushing.
    assert!(tokio::time::timeout(
        Duration::from_secs(3),
        run_and_assert_sink_compliance(sink, stream::iter(events), &AWS_SINK_TAGS)
    )
    .await
    .is_ok());

    let keys = get_keys(&bucket, prefix).await;
    assert_eq!(keys.len(), 1);

    let mut response_lines: Vec<String> = Vec::new();
    let mut key_stream = stream::iter(keys);
    while let Some(key) = key_stream.next().await {
        let obj = get_object(&bucket, key).await;
        response_lines.append(&mut get_lines(obj).await);
    }

    assert_eq!(lines, response_lines); // if all events are received, and lines.len() < batch size, then a flush was performed.
}

async fn client() -> S3Client {
    let auth = AwsAuthentication::test_auth();
    let region = RegionOrEndpoint::with_both("us-east-1", s3_address());
    let proxy = ProxyConfig::default();
    let tls_options = None;
    create_client::<S3ClientBuilder>(
        &auth,
        region.region(),
        region.endpoint(),
        &proxy,
        &tls_options,
    )
    .await
    .unwrap()
}

fn config(bucket: &str, batch_size: usize) -> S3SinkConfig {
    let mut batch = BatchConfig::default();
    batch.max_events = Some(batch_size);
    batch.timeout_secs = Some(5.0);

    S3SinkConfig {
        bucket: bucket.to_string(),
        key_prefix: random_string(10) + "/date=%F",
        filename_time_format: default_filename_time_format(),
        filename_append_uuid: true,
        filename_extension: None,
        options: S3Options::default(),
        region: RegionOrEndpoint::with_both("us-east-1", s3_address()),
        encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
        compression: Compression::None,
        batch,
        request: TowerRequestConfig::default(),
        tls: Default::default(),
        auth: Default::default(),
        acknowledgements: Default::default(),
        timezone: Default::default(),
    }
}

fn make_events_batch(
    len: usize,
    count: usize,
) -> (
    Vec<String>,
    impl Stream<Item = EventArray>,
    BatchStatusReceiver,
) {
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let (lines, events) = random_lines_with_stream(len, count, Some(batch));

    (lines, events.map(Into::into), receiver)
}

async fn create_bucket(bucket: &str, object_lock_enabled: bool) {
    match client()
        .await
        .create_bucket()
        .bucket(bucket.to_string())
        .object_lock_enabled_for_bucket(object_lock_enabled)
        .send()
        .await
    {
        Ok(_) => {}
        Err(err) => match err {
            SdkError::ServiceError(inner) => match &inner.err() {
                CreateBucketError::BucketAlreadyOwnedByYou(_) => {}
                err => panic!("Failed to create bucket: {:?}", err),
            },
            err => panic!("Failed to create bucket: {:?}", err),
        },
    }
}

async fn list_objects(bucket: &str, prefix: String) -> Option<Vec<aws_sdk_s3::types::Object>> {
    let prefix = prefix.split('/').next().unwrap().to_string();

    client()
        .await
        .list_objects_v2()
        .bucket(bucket.to_string())
        .prefix(prefix)
        .send()
        .await
        .unwrap()
        .contents
}

async fn get_keys(bucket: &str, prefix: String) -> Vec<String> {
    list_objects(bucket, prefix)
        .await
        .unwrap()
        .into_iter()
        .map(|obj| obj.key.unwrap())
        .collect()
}

async fn get_object(bucket: &str, key: String) -> GetObjectOutput {
    client()
        .await
        .get_object()
        .bucket(bucket.to_string())
        .key(key)
        .send()
        .await
        .unwrap()
}

async fn get_lines(obj: GetObjectOutput) -> Vec<String> {
    let body = get_object_output_body(obj).await;
    let buf_read = BufReader::new(body);
    buf_read.lines().map(|l| l.unwrap()).collect()
}

async fn get_gzipped_lines(obj: GetObjectOutput) -> Vec<String> {
    let body = get_object_output_body(obj).await;
    let buf_read = BufReader::new(MultiGzDecoder::new(body));
    buf_read.lines().map(|l| l.unwrap()).collect()
}

async fn get_zstd_lines(obj: GetObjectOutput) -> Vec<String> {
    let body = get_object_output_body(obj).await;
    let decoder = zstd::Decoder::new(body).expect("zstd decoder initialization failed");
    let buf_read = BufReader::new(decoder);
    buf_read.lines().map(|l| l.unwrap()).collect()
}

async fn get_object_output_body(obj: GetObjectOutput) -> impl std::io::Read {
    obj.body.collect().await.unwrap().reader()
}
