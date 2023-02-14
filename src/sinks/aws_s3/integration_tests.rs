#![cfg(all(test, feature = "aws-s3-integration-tests"))]

use std::{
    io::{BufRead, BufReader},
    time::Duration,
};

use aws_sdk_s3::{
    error::CreateBucketErrorKind,
    model::{
        DefaultRetention, ObjectLockConfiguration, ObjectLockEnabled, ObjectLockRetentionMode,
        ObjectLockRule,
    },
    output::GetObjectOutput,
    types::SdkError,
    Client as S3Client,
};
use bytes::Buf;
use codecs::{encoding::FramingConfig, TextSerializerConfig};
use flate2::read::MultiGzDecoder;
use futures::{stream, Stream};
use similar_asserts::assert_eq;
use tokio_stream::StreamExt;
use vector_core::{
    config::proxy::ProxyConfig,
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, EventArray, LogEvent},
};

use super::S3SinkConfig;
use crate::test_util::components::{run_and_assert_sink_error, COMPONENT_ERROR_TAGS};
use crate::{
    aws::{create_client, AwsAuthentication, RegionOrEndpoint},
    common::s3::S3ClientBuilder,
    config::SinkContext,
    sinks::{
        s3_common::config::{S3Options, S3ServerSideEncryption},
        util::{BatchConfig, Compression, TowerRequestConfig},
    },
    test_util::{
        components::{run_and_assert_sink_compliance, AWS_SINK_TAGS},
        random_lines_with_stream, random_string,
    },
};

fn s3_address() -> String {
    std::env::var("S3_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
}

#[tokio::test]
async fn s3_insert_message_into_with_flat_key_prefix() {
    let cx = SinkContext::new_test();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let mut config = config(&bucket, 1000000);
    config.key_prefix = Some("test-prefix".to_string());
    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service).unwrap();

    let (lines, events, receiver) = make_events_batch(100, 10);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = get_keys(&bucket, prefix.unwrap()).await;
    assert_eq!(keys.len(), 1);

    let key = keys[0].clone();
    let key_parts = key.split('/');
    assert!(key_parts.count() == 1);
    assert!(key.starts_with("test-prefix"));
    assert!(key.ends_with(".log"));

    let obj = get_object(&bucket, key).await;
    assert_eq!(obj.content_encoding, Some("identity".to_string()));

    let response_lines = get_lines(obj).await;
    assert_eq!(lines, response_lines);
}

#[tokio::test]
async fn s3_insert_message_into_with_folder_key_prefix() {
    let cx = SinkContext::new_test();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let mut config = config(&bucket, 1000000);
    config.key_prefix = Some("test-prefix/".to_string());
    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service).unwrap();

    let (lines, events, receiver) = make_events_batch(100, 10);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = get_keys(&bucket, prefix.unwrap()).await;
    assert_eq!(keys.len(), 1);

    let key = keys[0].clone();
    let key_parts = key.split('/').collect::<Vec<_>>();
    assert!(key_parts.len() == 2);
    assert!(*key_parts.first().unwrap() == "test-prefix");
    assert!(key.ends_with(".log"));

    let obj = get_object(&bucket, key).await;
    assert_eq!(obj.content_encoding, Some("identity".to_string()));

    let response_lines = get_lines(obj).await;
    assert_eq!(lines, response_lines);
}

#[tokio::test]
async fn s3_insert_message_into_with_ssekms_key_id() {
    let cx = SinkContext::new_test();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let mut config = config(&bucket, 1000000);
    config.key_prefix = Some("test-prefix".to_string());
    let prefix = config.key_prefix.clone();
    config.options.server_side_encryption = Some(S3ServerSideEncryption::AwsKms);
    config.options.ssekms_key_id = Some("alias/aws/s3".to_string());

    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service).unwrap();

    let (lines, events, receiver) = make_events_batch(100, 10);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = get_keys(&bucket, prefix.unwrap()).await;
    assert_eq!(keys.len(), 1);

    let key = keys[0].clone();
    let key_parts = key.split('/');
    assert!(key_parts.count() == 1);
    assert!(key.starts_with("test-prefix"));
    assert!(key.ends_with(".log"));

    let obj = get_object(&bucket, key).await;
    assert_eq!(obj.content_encoding, Some("identity".to_string()));

    let response_lines = get_lines(obj).await;
    assert_eq!(lines, response_lines);
}

#[tokio::test]
async fn s3_rotate_files_after_the_buffer_size_is_reached() {
    let cx = SinkContext::new_test();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let config = S3SinkConfig {
        key_prefix: Some(format!("{}/{}", random_string(10), "{{i}}")),
        filename_time_format: Some("waitsforfullbatch".into()),
        filename_append_uuid: Some(false),
        ..config(&bucket, 10)
    };
    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service).unwrap();

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

    let keys = get_keys(&bucket, prefix.unwrap()).await;
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
    let cx = SinkContext::new_test();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let batch_size = 1_000;
    let batch_multiplier = 3;
    let config = S3SinkConfig {
        compression: Compression::gzip_default(),
        filename_time_format: Some("%s%f".into()),
        ..config(&bucket, batch_size)
    };

    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service).unwrap();

    let (lines, events, receiver) = make_events_batch(100, batch_size * batch_multiplier);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = get_keys(&bucket, prefix.unwrap()).await;
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

// NOTE: this test doesn't actually validate anything because localstack
// doesn't enforce the required Content-MD5 header on the request for
// buckets with object lock enabled
// https://github.com/localstack/localstack/issues/4166
#[tokio::test]
async fn s3_insert_message_into_object_lock() {
    let cx = SinkContext::new_test();

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
    let sink = config.build_processor(service).unwrap();

    let (lines, events, receiver) = make_events_batch(100, 10);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Delivered);

    let keys = get_keys(&bucket, prefix.unwrap()).await;
    assert_eq!(keys.len(), 1);

    let key = keys[0].clone();
    assert!(key.ends_with(".log"));

    let obj = get_object(&bucket, key).await;
    assert_eq!(obj.content_encoding, Some("identity".to_string()));

    let response_lines = get_lines(obj).await;
    assert_eq!(lines, response_lines);
}

#[tokio::test]
async fn acknowledges_failures() {
    let cx = SinkContext::new_test();

    let bucket = uuid::Uuid::new_v4().to_string();

    create_bucket(&bucket, false).await;

    let mut config = config(&bucket, 1);
    // Break the bucket name
    config.bucket = format!("BREAK{}IT", config.bucket);
    let prefix = config.key_prefix.clone();
    let service = config.create_service(&cx.globals.proxy).await.unwrap();
    let sink = config.build_processor(service).unwrap();

    let (_lines, events, receiver) = make_events_batch(1, 1);
    run_and_assert_sink_error(sink, events, &COMPONENT_ERROR_TAGS).await;
    assert_eq!(receiver.await, BatchStatus::Rejected);

    let objects = list_objects(&bucket, prefix.unwrap()).await;
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

async fn client() -> S3Client {
    let auth = AwsAuthentication::test_auth();
    let region = RegionOrEndpoint::with_both("minio", s3_address());
    let proxy = ProxyConfig::default();
    let tls_options = None;
    create_client::<S3ClientBuilder>(
        &auth,
        region.region(),
        region.endpoint().unwrap(),
        &proxy,
        &tls_options,
        true,
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
        key_prefix: Some(random_string(10) + "/date=%F"),
        filename_time_format: None,
        filename_append_uuid: None,
        filename_extension: None,
        options: S3Options::default(),
        region: RegionOrEndpoint::with_both("minio", s3_address()),
        encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
        compression: Compression::None,
        batch,
        request: TowerRequestConfig::default(),
        tls: Default::default(),
        auth: Default::default(),
        acknowledgements: Default::default(),
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
            SdkError::ServiceError(inner) => match &inner.err().kind {
                CreateBucketErrorKind::BucketAlreadyOwnedByYou(_) => {}
                err => panic!("Failed to create bucket: {:?}", err),
            },
            err => panic!("Failed to create bucket: {:?}", err),
        },
    }
}

async fn list_objects(bucket: &str, prefix: String) -> Option<Vec<aws_sdk_s3::model::Object>> {
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

async fn get_object_output_body(obj: GetObjectOutput) -> impl std::io::Read {
    obj.body.collect().await.unwrap().reader()
}
