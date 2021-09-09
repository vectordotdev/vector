#[cfg(feature = "aws-s3-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use crate::config::SinkContext;
    use crate::rusoto::RegionOrEndpoint;
    use crate::sinks::aws_s3::config::{build_healthcheck, create_client, Encoding, S3Options};
    use crate::sinks::aws_s3::S3SinkConfig;
    use crate::sinks::util::BatchConfig;
    use crate::sinks::util::Compression;
    use crate::sinks::util::TowerRequestConfig;
    use crate::test_util::{random_lines_with_stream, random_string};
    use bytes::{Buf, BytesMut};
    use flate2::read::MultiGzDecoder;
    use futures::{stream, Stream};
    use pretty_assertions::assert_eq;
    use rusoto_core::{region::Region, RusotoError};
    use rusoto_s3::S3Client;
    use rusoto_s3::S3;
    use std::io::{BufRead, BufReader};
    use tokio_stream::StreamExt;
    use vector_core::config::proxy::ProxyConfig;
    use vector_core::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, LogEvent};

    #[tokio::test]
    async fn s3_insert_message_into() {
        let cx = SinkContext::new_test();

        let bucket = uuid::Uuid::new_v4().to_string();

        create_bucket(&bucket, false).await;

        let config = config(&bucket, 1000000);
        let prefix = config.key_prefix.clone();
        let client = create_client(&config.region, &config.auth, None, &cx.globals.proxy).unwrap();
        let sink = config.build_processor(client, cx).unwrap();

        let (lines, events, mut receiver) = make_events_batch(100, 10);
        sink.run(events).await.unwrap();
        // It's possible that the internal machinery of the sink is still
        // spinning up. We pause here to give the batch time to wind
        // through. Waiting is preferable to adding synchronization into the
        // actual sync code for the sole benefit of these tests.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

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
    async fn s3_rotate_files_after_the_buffer_size_is_reached() {
        let cx = SinkContext::new_test();

        let bucket = uuid::Uuid::new_v4().to_string();

        create_bucket(&bucket, false).await;

        let config = S3SinkConfig {
            key_prefix: Some(format!("{}/{}", random_string(10), "{{i}}")),
            filename_time_format: Some("waitsforfullbatch".into()),
            filename_append_uuid: Some(false),
            ..config(&bucket, 1010)
        };
        let prefix = config.key_prefix.clone();
        let client = create_client(&config.region, &config.auth, None, &cx.globals.proxy).unwrap();
        let sink = config.build_processor(client, cx).unwrap();

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

        sink.run(stream::iter(events)).await.unwrap();
        // It's possible that the internal machinery of the sink is still
        // spinning up. We pause here to give the batch time to wind
        // through. Waiting is preferable to adding synchronization into the
        // actual sync code for the sole benefit of these tests.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

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
        let cx = SinkContext::new_test();

        let bucket = uuid::Uuid::new_v4().to_string();

        create_bucket(&bucket, false).await;

        let batch_size = 1_000;
        let config = S3SinkConfig {
            compression: Compression::gzip_default(),
            filename_time_format: Some("%s%f".into()),
            ..config(&bucket, batch_size)
        };

        let prefix = config.key_prefix.clone();
        let client = create_client(&config.region, &config.auth, None, &cx.globals.proxy).unwrap();
        let sink = config.build_processor(client, cx).unwrap();

        let (lines, events, mut receiver) = make_events_batch(100, batch_size);
        sink.run(events).await.unwrap();
        // It's possible that the internal machinery of the sink is still
        // spinning up. We pause here to give the batch time to wind
        // through. Waiting is preferable to adding synchronization into the
        // actual sync code for the sole benefit of these tests.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let keys = get_keys(&bucket, prefix.unwrap()).await;
        assert_eq!(keys.len(), 1);

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
        let client = create_client(&config.region, &config.auth, None, &cx.globals.proxy).unwrap();
        let sink = config.build_processor(client, cx).unwrap();

        let (lines, events, mut receiver) = make_events_batch(100, 10);
        sink.run(events).await.unwrap();
        // It's possible that the internal machinery of the sink is still
        // spinning up. We pause here to give the batch time to wind
        // through. Waiting is preferable to adding synchronization into the
        // actual sync code for the sole benefit of these tests.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

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
        let client = create_client(&config.region, &config.auth, None, &cx.globals.proxy).unwrap();
        let sink = config.build_processor(client, cx).unwrap();

        let (_lines, events, mut receiver) = make_events_batch(1, 1);
        sink.run(events).await.unwrap();
        // It's possible that the internal machinery of the sink is still
        // spinning up. We pause here to give the batch time to wind
        // through. Waiting is preferable to adding synchronization into the
        // actual sync code for the sole benefit of these tests.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Errored));

        let objects = list_objects(&bucket, prefix.unwrap()).await;
        assert_eq!(objects, None);
    }

    #[tokio::test]
    async fn s3_healthchecks() {
        let bucket = uuid::Uuid::new_v4().to_string();

        create_bucket(&bucket, false).await;

        let config = config(&bucket, 1);
        let client =
            create_client(&config.region, &config.auth, None, &ProxyConfig::from_env()).unwrap();
        build_healthcheck(bucket, client).unwrap();
    }

    #[tokio::test]
    async fn s3_healthchecks_invalid_bucket() {
        let config = config("s3_healthchecks_invalid_bucket", 1);
        let client =
            create_client(&config.region, &config.auth, None, &ProxyConfig::from_env()).unwrap();
        assert!(build_healthcheck(config.bucket, client)
            .unwrap()
            .await
            .is_err());
    }

    fn client() -> S3Client {
        let region = Region::Custom {
            name: "minio".to_owned(),
            endpoint: "http://localhost:4566".to_owned(),
        };

        use rusoto_core::HttpClient;
        use rusoto_credential::StaticProvider;

        let p = StaticProvider::new_minimal("test-access-key".into(), "test-secret-key".into());
        let d = HttpClient::new().unwrap();

        S3Client::new_with(d, p, region)
    }

    fn config(bucket: &str, batch_size: usize) -> S3SinkConfig {
        S3SinkConfig {
            bucket: bucket.to_string(),
            key_prefix: Some(random_string(10) + "/date=%F"),
            filename_time_format: None,
            filename_append_uuid: None,
            filename_extension: None,
            options: S3Options::default(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:4566".to_owned()),
            encoding: Encoding::Text.into(),
            compression: Compression::None,
            batch: BatchConfig {
                max_bytes: Some(batch_size),
                timeout_secs: Some(5),
                ..Default::default()
            },
            request: TowerRequestConfig::default(),
            assume_role: None,
            auth: Default::default(),
        }
    }

    fn make_events_batch(
        len: usize,
        count: usize,
    ) -> (Vec<String>, impl Stream<Item = Event>, BatchStatusReceiver) {
        let (lines, events) = random_lines_with_stream(len, count, None);

        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let events = events.map(move |event| event.into_log().with_batch_notifier(&batch).into());

        (lines, events, receiver)
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
