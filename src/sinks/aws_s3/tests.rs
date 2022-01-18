#[cfg(feature = "aws-s3-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use std::{
        io::{BufRead, BufReader},
        time::Duration,
    };

    use bytes::{Buf, BytesMut};
    use flate2::read::MultiGzDecoder;
    use futures::{stream, Stream};
    use pretty_assertions::assert_eq;
    use rusoto_core::{region::Region, RusotoError};
    use rusoto_s3::{S3Client, S3};
    use tokio_stream::StreamExt;
    use vector_core::{
        config::proxy::ProxyConfig,
        event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, EventArray, LogEvent},
    };

    use crate::{
        aws::rusoto::RegionOrEndpoint,
        config::SinkContext,
        sinks::{
            aws_s3::S3SinkConfig,
            s3_common::config::S3Options,
            util::{encoding::StandardEncodings, BatchConfig, Compression, TowerRequestConfig},
        },
        test_util::{random_lines_with_stream, random_string},
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
        let service = config.create_service(&cx.globals.proxy).unwrap();
        let sink = config.build_processor(service, cx).unwrap();

        let (lines, events, receiver) = make_events_batch(100, 10);
        sink.run(events).await.unwrap();
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
        let service = config.create_service(&cx.globals.proxy).unwrap();
        let sink = config.build_processor(service, cx).unwrap();

        let (lines, events, receiver) = make_events_batch(100, 10);
        sink.run(events).await.unwrap();
        assert_eq!(receiver.await, BatchStatus::Delivered);

        let keys = get_keys(&bucket, prefix.unwrap()).await;
        assert_eq!(keys.len(), 1);

        let key = keys[0].clone();
        let key_parts = key.split('/').collect::<Vec<_>>();
        assert!(key_parts.len() == 2);
        assert!(*key_parts.get(0).unwrap() == "test-prefix");
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
        let service = config.create_service(&cx.globals.proxy).unwrap();
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
            e.insert("i", format!("{}", i));
            Event::from(e)
        });

        sink.run_events(events).await.unwrap();

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
        let service = config.create_service(&cx.globals.proxy).unwrap();
        let sink = config.build_processor(service, cx).unwrap();

        let (lines, events, receiver) = make_events_batch(100, batch_size * batch_multiplier);
        sink.run(events).await.unwrap();
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
            .put_object_lock_configuration(rusoto_s3::PutObjectLockConfigurationRequest {
                bucket: bucket.to_string(),
                object_lock_configuration: Some(rusoto_s3::ObjectLockConfiguration {
                    object_lock_enabled: Some(String::from("Enabled")),
                    rule: Some(rusoto_s3::ObjectLockRule {
                        default_retention: Some(rusoto_s3::DefaultRetention {
                            days: Some(1),
                            mode: Some(String::from("GOVERNANCE")),
                            years: None,
                        }),
                    }),
                }),
                ..Default::default()
            })
            .await
            .unwrap();

        let config = config(&bucket, 1000000);
        let prefix = config.key_prefix.clone();
        let service = config.create_service(&cx.globals.proxy).unwrap();
        let sink = config.build_processor(service, cx).unwrap();

        let (lines, events, receiver) = make_events_batch(100, 10);
        sink.run(events).await.unwrap();
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
        let service = config.create_service(&cx.globals.proxy).unwrap();
        let sink = config.build_processor(service, cx).unwrap();

        let (_lines, events, receiver) = make_events_batch(1, 1);
        sink.run(events).await.unwrap();
        assert_eq!(receiver.await, BatchStatus::Rejected);

        let objects = list_objects(&bucket, prefix.unwrap()).await;
        assert_eq!(objects, None);
    }

    #[tokio::test]
    async fn s3_healthchecks() {
        let bucket = uuid::Uuid::new_v4().to_string();

        create_bucket(&bucket, false).await;

        let config = config(&bucket, 1);
        let service = config.create_service(&ProxyConfig::from_env()).unwrap();
        config.build_healthcheck(service.client()).unwrap();
    }

    #[tokio::test]
    async fn s3_healthchecks_invalid_bucket() {
        let config = config("s3_healthchecks_invalid_bucket", 1);
        let service = config.create_service(&ProxyConfig::from_env()).unwrap();
        assert!(config
            .build_healthcheck(service.client())
            .unwrap()
            .await
            .is_err());
    }

    fn client() -> S3Client {
        let region = Region::Custom {
            name: "minio".to_owned(),
            endpoint: s3_address(),
        };

        use rusoto_core::HttpClient;
        use rusoto_credential::StaticProvider;

        let p = StaticProvider::new_minimal("test-access-key".into(), "test-secret-key".into());
        let d = HttpClient::new().unwrap();

        S3Client::new_with(d, p, region)
    }

    fn config(bucket: &str, batch_size: usize) -> S3SinkConfig {
        let mut batch = BatchConfig::default();
        batch.max_events = Some(batch_size);
        batch.timeout_secs = Some(5);

        S3SinkConfig {
            bucket: bucket.to_string(),
            key_prefix: Some(random_string(10) + "/date=%F"),
            filename_time_format: None,
            filename_append_uuid: None,
            filename_extension: None,
            options: S3Options::default(),
            region: RegionOrEndpoint::with_endpoint(s3_address()),
            encoding: StandardEncodings::Text.into(),
            compression: Compression::None,
            batch,
            request: TowerRequestConfig::default(),
            assume_role: None,
            auth: Default::default(),
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
        use rusoto_s3::{CreateBucketError, CreateBucketRequest};

        let req = CreateBucketRequest {
            bucket: bucket.to_string(),
            object_lock_enabled_for_bucket: Some(object_lock_enabled),
            ..Default::default()
        };

        match client().create_bucket(req).await {
            Ok(_) | Err(RusotoError::Service(CreateBucketError::BucketAlreadyOwnedByYou(_))) => {}
            Err(e) => match e {
                RusotoError::Unknown(resp) => {
                    let body = String::from_utf8_lossy(&resp.body[..]);
                    panic!("Couldn't create bucket: {:?}; Body {}", resp, body);
                }
                _ => panic!("Couldn't create bucket: {}", e),
            },
        }
    }

    async fn list_objects(bucket: &str, prefix: String) -> Option<Vec<rusoto_s3::Object>> {
        let prefix = prefix.split('/').next().unwrap().to_string();

        client()
            .list_objects_v2(rusoto_s3::ListObjectsV2Request {
                bucket: bucket.to_string(),
                prefix: Some(prefix),
                ..Default::default()
            })
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

    async fn get_object(bucket: &str, key: String) -> rusoto_s3::GetObjectOutput {
        client()
            .get_object(rusoto_s3::GetObjectRequest {
                bucket: bucket.to_string(),
                key,
                ..Default::default()
            })
            .await
            .unwrap()
    }

    async fn get_lines(obj: rusoto_s3::GetObjectOutput) -> Vec<String> {
        let body = get_object_output_body(obj).await;
        let buf_read = BufReader::new(body);
        buf_read.lines().map(|l| l.unwrap()).collect()
    }

    async fn get_gzipped_lines(obj: rusoto_s3::GetObjectOutput) -> Vec<String> {
        let body = get_object_output_body(obj).await;
        let buf_read = BufReader::new(MultiGzDecoder::new(body));
        buf_read.lines().map(|l| l.unwrap()).collect()
    }

    async fn get_object_output_body(obj: rusoto_s3::GetObjectOutput) -> impl std::io::Read {
        let bytes = obj
            .body
            .unwrap()
            .fold(BytesMut::new(), |mut store, bytes| {
                store.extend_from_slice(&bytes.unwrap());
                store
            })
            .await;
        bytes.freeze().reader()
    }
}
