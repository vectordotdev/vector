#[cfg(feature = "aws-s3-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        assert_downcast_matches,
        test_util::{random_lines_with_stream, random_string},
    };
    use bytes::{Buf, BytesMut};
    use flate2::read::MultiGzDecoder;
    use futures::Stream;
    use pretty_assertions::assert_eq;
    use rusoto_core::region::Region;
    use std::io::{BufRead, BufReader};
    use vector_core::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, LogEvent};

    #[tokio::test]
    async fn s3_insert_message_into() {
        let cx = SinkContext::new_test();

        let bucket = uuid::Uuid::new_v4().to_string();

        create_bucket(&bucket, false).await;

        let config = config(&bucket, 1000000);
        let prefix = config.key_prefix.clone();
        let client = config.create_client(&cx.globals.proxy).unwrap();
        let sink = config.new(client, cx).unwrap();

        let (lines, events, mut receiver) = make_events_batch(100, 10);
        sink.run(events).await.unwrap();
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
        let client = config.create_client(&cx.globals.proxy).unwrap();
        let sink = config.new(client, cx).unwrap();

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

        let keys = get_keys(&bucket, prefix.unwrap()).await;
        assert_eq!(keys.len(), 3);

        let response_lines = stream::iter(keys)
            .fold(Vec::new(), |mut acc, key| async {
                acc.push(get_lines(get_object(&bucket, key).await).await);
                acc
            })
            .await;

        assert_eq!(&lines[00..10], response_lines[0].as_slice());
        assert_eq!(&lines[10..20], response_lines[1].as_slice());
        assert_eq!(&lines[20..30], response_lines[2].as_slice());
    }

    #[tokio::test]
    async fn s3_gzip() {
        let cx = SinkContext::new_test();

        let bucket = uuid::Uuid::new_v4().to_string();

        create_bucket(&bucket, false).await;

        let config = S3SinkConfig {
            compression: Compression::gzip_default(),
            filename_time_format: Some("%s%f".into()),
            ..config(&bucket, 10000)
        };

        let prefix = config.key_prefix.clone();
        let client = config.create_client(&cx.globals.proxy).unwrap();
        let sink = config.new(client, cx).unwrap();

        let (lines, events, mut receiver) = make_events_batch(100, 500);
        sink.run(events).await.unwrap();
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let keys = get_keys(&bucket, prefix.unwrap()).await;
        assert_eq!(keys.len(), 6);

        let response_lines = stream::iter(keys).fold(Vec::new(), |mut acc, key| async {
            assert!(key.ends_with(".log.gz"));

            let obj = get_object(&bucket, key).await;
            assert_eq!(obj.content_encoding, Some("gzip".to_string()));

            acc.append(&mut get_gzipped_lines(obj).await);
            acc
        });

        assert_eq!(lines, response_lines.await);
    }

    // NOTE: this test doesn't actually validate anything because localstack doesn't enforce the
    // required Content-MD5 header on the request for buckets with object lock enabled
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
        let client = config.create_client(&cx.globals.proxy).unwrap();
        let sink = config.new(client, cx).unwrap();

        let (lines, events, mut receiver) = make_events_batch(100, 10);
        sink.run(events).await.unwrap();
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
        let client = config.create_client(&cx.globals.proxy).unwrap();
        let sink = config.new(client, cx).unwrap();

        let (_lines, events, mut receiver) = make_events_batch(1, 1);
        sink.run(events).await.unwrap();
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Errored));

        let objects = list_objects(&bucket, prefix.unwrap()).await;
        assert_eq!(objects, None);
    }

    #[tokio::test]
    async fn s3_healthchecks() {
        let bucket = uuid::Uuid::new_v4().to_string();

        create_bucket(&bucket, false).await;

        let config = config(&bucket, 1);
        let client = config.create_client(&ProxyConfig::from_env()).unwrap();
        config.healthcheck(client).await.unwrap();
    }

    #[tokio::test]
    async fn s3_healthchecks_invalid_bucket() {
        let config = config("s3_healthchecks_invalid_bucket", 1);

        let client = config.create_client(&ProxyConfig::from_env()).unwrap();
        assert_downcast_matches!(
            config.healthcheck(client).await.unwrap_err(),
            HealthcheckError,
            HealthcheckError::UnknownBucket { .. }
        );
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
            key_prefix: Some(random_string(10) + "/date=%F/"),
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
            .fold(BytesMut::new(), |mut store, bytes| async move {
                store.extend_from_slice(&bytes.unwrap());
                store
            })
            .await;
        bytes.freeze().reader()
    }
}
