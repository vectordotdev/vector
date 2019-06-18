use crate::{
    buffers::Acker,
    event::{self, Event, ValueKind},
    region::RegionOrEndpoint,
    sinks::util::{
        retries::{FixedRetryPolicy, RetryLogic},
        Batch, BatchServiceSink, Buffer, Partition, SinkExt,
    },
};
use bytes::Bytes;
use chrono::Utc;
use futures::{Future, Poll, Sink};
use rusoto_core::{Region, RusotoFuture};
use rusoto_s3::{
    HeadBucketError, HeadBucketRequest, PutObjectError, PutObjectOutput, PutObjectRequest,
    S3Client, S3,
};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::time::Duration;
use tokio_trace::field;
use tokio_trace_futures::{Instrument, Instrumented};
use tower::{Service, ServiceBuilder};
use uuid::Uuid;

#[derive(Clone)]
pub struct S3Sink {
    client: S3Client,
    key_prefix: String,
    bucket: String,
    gzip: bool,
    filename_time_format: String,
    filename_append_uuid: bool,
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct S3SinkConfig {
    pub bucket: String,
    pub key_prefix: Option<String>,
    pub filename_time_format: Option<String>,
    pub filename_append_uuid: Option<bool>,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub batch_size: Option<usize>,
    pub compression: Compression,
    pub batch_timeout: Option<u64>,
    pub encoding: Option<Encoding>,

    // Tower Request based configuration
    pub request_in_flight_limit: Option<usize>,
    pub request_timeout_secs: Option<u64>,
    pub request_rate_limit_duration_secs: Option<u64>,
    pub request_rate_limit_num: Option<u64>,
    pub request_retry_attempts: Option<usize>,
    pub request_retry_backoff_secs: Option<u64>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Ndjson,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    Gzip,
    None,
}

pub struct S3Buffer {
    inner: Buffer,
    key: Option<Bytes>,
}

#[derive(Clone)]
pub struct InnerBuffer {
    pub(self) inner: Vec<u8>,
    key: Bytes,
}

impl Partition for InnerBuffer {
    fn partition(&self) -> Bytes {
        self.key.clone()
    }
}

impl S3Buffer {
    pub fn new(gzip: bool) -> Self {
        Self {
            inner: Buffer::new(gzip),
            key: None,
        }
    }
}

impl Batch for S3Buffer {
    type Input = InnerBuffer;
    type Output = InnerBuffer;

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn push(&mut self, item: Self::Input) {
        let partition = item.partition();
        self.key = Some(partition);
        self.inner.push(&item.inner[..])
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn fresh(&self) -> Self {
        Self {
            inner: self.inner.fresh(),
            key: None,
        }
    }

    fn finish(mut self) -> Self::Output {
        let key = self.key.take().unwrap();
        let inner = self.inner.finish();
        InnerBuffer { inner, key }
    }

    fn num_items(&self) -> usize {
        self.inner.num_items()
    }
}

impl Default for Compression {
    fn default() -> Self {
        Compression::Gzip
    }
}

#[typetag::serde(name = "aws_s3")]
impl crate::topology::config::SinkConfig for S3SinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let sink = S3Sink::new(self, acker)?;
        let healthcheck = S3Sink::healthcheck(self)?;

        Ok((sink, healthcheck))
    }
}

impl S3Sink {
    pub fn new(config: &S3SinkConfig, acker: Acker) -> Result<super::RouterSink, String> {
        let timeout = config.request_timeout_secs.unwrap_or(60);
        let in_flight_limit = config.request_in_flight_limit.unwrap_or(25);
        let rate_limit_duration = config.request_rate_limit_duration_secs.unwrap_or(1);
        let rate_limit_num = config.request_rate_limit_num.unwrap_or(25);
        let retry_attempts = config.request_retry_attempts.unwrap_or(usize::max_value());
        let retry_backoff_secs = config.request_retry_backoff_secs.unwrap_or(1);
        let encoding = config.encoding.clone();

        let policy = FixedRetryPolicy::new(
            retry_attempts,
            Duration::from_secs(retry_backoff_secs),
            S3RetryLogic,
        );

        let batch_timeout = config.batch_timeout.unwrap_or(300);
        let compression = match config.compression {
            Compression::Gzip => true,
            Compression::None => false,
        };
        let batch_size = config.batch_size.unwrap_or(bytesize::mib(10u64) as usize);
        let filename_time_format = config.filename_time_format.clone().unwrap_or("%s".into());
        let filename_append_uuid = config.filename_append_uuid.unwrap_or(true);

        let key_prefix = config
            .key_prefix
            .clone()
            .unwrap_or_else(|| "date=%F/".into());

        let region = config.region.clone();
        let s3 = S3Sink {
            client: Self::create_client(region.try_into()?),
            key_prefix: key_prefix.clone(),
            bucket: config.bucket.clone(),
            gzip: compression,
            filename_time_format,
            filename_append_uuid,
        };

        let svc = ServiceBuilder::new()
            .concurrency_limit(in_flight_limit)
            .rate_limit(rate_limit_num, Duration::from_secs(rate_limit_duration))
            .retry(policy)
            .timeout(Duration::from_secs(timeout))
            .service(s3);

        let sink = BatchServiceSink::new(svc, acker)
            .partitioned_batched_with_min(
                S3Buffer::new(compression),
                batch_size,
                Duration::from_secs(batch_timeout),
            )
            .with(move |e| encode_event(e, &key_prefix, &encoding));

        Ok(Box::new(sink))
    }

    pub fn healthcheck(config: &S3SinkConfig) -> Result<super::Healthcheck, String> {
        let region = config.region.clone();
        let client = Self::create_client(region.try_into()?);

        let request = HeadBucketRequest {
            bucket: config.bucket.clone(),
        };

        let response = client.head_bucket(request);

        let healthcheck = response.map_err(|err| match err {
            HeadBucketError::Unknown(response) => match response.status {
                http::status::StatusCode::FORBIDDEN => "Invalid credentials".to_string(),
                http::status::StatusCode::NOT_FOUND => "Unknown bucket".to_string(),
                status => format!("Unknown error: Status code: {}", status),
            },
            err => err.to_string(),
        });

        Ok(Box::new(healthcheck))
    }

    pub fn create_client(region: Region) -> S3Client {
        // Hack around the fact that rusoto will not pick up runtime
        // env vars. This is designed to only for test purposes use
        // static credentials.
        #[cfg(not(test))]
        {
            S3Client::new(region)
        }

        #[cfg(test)]
        {
            use rusoto_core::HttpClient;
            use rusoto_credential::StaticProvider;

            let p = StaticProvider::new_minimal("test-access-key".into(), "test-secret-key".into());
            let d = HttpClient::new().unwrap();

            S3Client::new_with(d, p, region)
        }
    }
}

impl Service<InnerBuffer> for S3Sink {
    type Response = PutObjectOutput;
    type Error = PutObjectError;
    type Future = Instrumented<RusotoFuture<PutObjectOutput, PutObjectError>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, body: InnerBuffer) -> Self::Future {
        let InnerBuffer { inner, key } = body;

        // TODO: pull the seconds from the last event
        let filename = {
            let seconds = Utc::now().format(&self.filename_time_format);

            if self.filename_append_uuid {
                let uuid = Uuid::new_v4();
                format!("{}-{}", seconds, uuid.to_hyphenated())
            } else {
                seconds.to_string()
            }
        };

        let extension = if self.gzip { "log.gz" } else { "log" };

        let key = String::from_utf8_lossy(&key[..]).into_owned();

        let key = format!("{}/{}.{}", key, filename, extension);

        debug!(
            message = "sending events.",
            bytes = &field::debug(inner.len()),
            bucket = &field::debug(&self.bucket),
            key = &field::debug(&key)
        );

        let request = PutObjectRequest {
            body: Some(inner.into()),
            bucket: self.bucket.clone(),
            key,
            content_encoding: if self.gzip {
                Some("gzip".to_string())
            } else {
                None
            },
            ..Default::default()
        };

        self.client
            .put_object(request)
            .instrument(info_span!("request"))
    }
}

#[derive(Debug, Clone)]
struct S3RetryLogic;

impl RetryLogic for S3RetryLogic {
    type Error = PutObjectError;
    type Response = PutObjectOutput;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            PutObjectError::Unknown(res) if res.status.is_server_error() => true,
            _ => false,
        }
    }
}

fn encode_event(
    event: Event,
    batch_time_format: &String,
    encoding: &Option<Encoding>,
) -> Result<InnerBuffer, ()> {
    let log = event.into_log();

    let key = if let Some(ValueKind::Timestamp(ts)) = log.get(&event::TIMESTAMP) {
        ts.format(batch_time_format).to_string()
    } else {
        Utc::now().format(batch_time_format).to_string()
    };

    match (encoding, log.is_structured()) {
        (&Some(Encoding::Ndjson), _) | (_, true) => serde_json::to_vec(&log.all_fields())
            .map(|mut b| {
                b.push(b'\n');
                b
            })
            .map(|b| InnerBuffer {
                inner: b,
                key: key.into(),
            })
            .map_err(|e| panic!("Error encoding: {}", e)),
        (&Some(Encoding::Text), _) | (_, false) => {
            let mut bytes = log
                .get(&event::MESSAGE)
                .map(|v| v.as_bytes().to_vec())
                .unwrap_or(Vec::new());
            bytes.push(b'\n');
            Ok(InnerBuffer {
                inner: bytes,
                key: key.into(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{self, Event};

    use std::collections::HashMap;

    #[test]
    fn s3_encode_event_non_structured() {
        let message = "hello world".to_string();
        let batch_time_format = "date=%F".to_string();
        let bytes = encode_event(message.clone().into(), &batch_time_format, &None).unwrap();

        let encoded_message = message + "\n";
        assert_eq!(&bytes.inner[..], encoded_message.as_bytes());
    }

    #[test]
    fn s3_encode_event_structured() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event
            .as_mut_log()
            .insert_explicit("key".into(), "value".into());

        let batch_time_format = "date=%F".to_string();
        let bytes = encode_event(event, &batch_time_format, &None).unwrap();

        let map: HashMap<String, String> = serde_json::from_slice(&bytes.inner[..]).unwrap();

        assert_eq!(map[&event::MESSAGE.to_string()], message);
        assert_eq!(map["key"], "value".to_string());
    }
}

#[cfg(feature = "s3-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::buffers::Acker;
    use crate::{
        event::Event,
        region::RegionOrEndpoint,
        sinks::aws_s3::{S3Sink, S3SinkConfig},
        test_util::{block_on, random_lines_with_stream, random_string},
    };
    use flate2::read::GzDecoder;
    use futures::{Future, Sink};
    use pretty_assertions::assert_eq;
    use rusoto_core::region::Region;
    use rusoto_s3::{S3Client, S3};
    use std::io::{BufRead, BufReader};

    const BUCKET: &str = "router-tests";

    #[test]
    fn s3_insert_message_into() {
        let config = config();
        let prefix = config.key_prefix.clone();
        let sink = S3Sink::new(&config, Acker::Null).unwrap();

        let (lines, events) = random_lines_with_stream(100, 10);

        let pump = sink.send_all(events);
        block_on(pump).unwrap();

        let keys = get_keys(prefix.unwrap());
        assert_eq!(keys.len(), 1);

        let key = keys[0].clone();
        assert!(key.ends_with(".log"));

        let obj = get_object(key);
        assert_eq!(obj.content_encoding, None);

        let response_lines = get_lines(obj);
        assert_eq!(lines, response_lines);
    }

    #[test]
    fn s3_rotate_files_after_the_buffer_size_is_reached() {
        ensure_bucket(&client());

        let config = S3SinkConfig {
            batch_size: Some(1000),
            filename_time_format: Some("%F%f".into()),
            ..config()
        };
        let prefix = config.key_prefix.clone();
        let sink = S3Sink::new(&config, Acker::Null).unwrap();

        let (lines, events) = random_lines_with_stream(100, 30);

        let pump = sink.send_all(events);
        block_on(pump).unwrap();

        let keys = get_keys(prefix.unwrap());
        assert_eq!(keys.len(), 3);

        let response_lines = keys
            .into_iter()
            .map(|key| get_lines(get_object(key)))
            .collect::<Vec<_>>();

        assert_eq!(&lines[00..10], response_lines[0].as_slice());
        assert_eq!(&lines[10..20], response_lines[1].as_slice());
        assert_eq!(&lines[20..30], response_lines[2].as_slice());
    }

    #[test]
    fn s3_waits_for_full_batch_or_timeout_before_sending() {
        ensure_bucket(&client());

        let config = S3SinkConfig {
            batch_size: Some(1000),
            filename_time_format: Some("%F%f".into()),
            ..config()
        };
        let prefix = config.key_prefix.clone();
        let sink = S3Sink::new(&config, Acker::Null).unwrap();

        let (lines, _) = random_lines_with_stream(100, 30);

        let (tx, rx) = futures::sync::mpsc::channel(1);
        let pump = sink.send_all(rx).map(|_| ()).map_err(|_| ());

        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(pump);

        let mut tx = tx.wait();
        for line in lines.iter().take(15) {
            tx.send(Event::from(line.as_str())).unwrap();
        }

        std::thread::sleep(std::time::Duration::from_millis(100));

        for line in lines.iter().skip(15) {
            tx.send(Event::from(line.as_str())).unwrap();
        }
        drop(tx);

        crate::test_util::shutdown_on_idle(rt);

        let keys = get_keys(prefix.unwrap());
        assert_eq!(keys.len(), 3);

        let response_lines = keys
            .into_iter()
            .map(|key| get_lines(get_object(key)))
            .collect::<Vec<_>>();

        assert_eq!(&lines[00..10], response_lines[0].as_slice());
        assert_eq!(&lines[10..20], response_lines[1].as_slice());
        assert_eq!(&lines[20..30], response_lines[2].as_slice());
    }

    #[test]
    fn s3_gzip() {
        ensure_bucket(&client());

        let config = S3SinkConfig {
            batch_size: Some(1000),
            compression: Compression::Gzip,
            filename_time_format: Some("%S%f".into()),
            ..config()
        };
        let prefix = config.key_prefix.clone();
        let sink = S3Sink::new(&config, Acker::Null).unwrap();

        let (lines, events) = random_lines_with_stream(100, 500);

        let pump = sink.send_all(events);
        block_on(pump).unwrap();

        let keys = get_keys(prefix.unwrap());
        assert_eq!(keys.len(), 2);

        let response_lines = keys
            .into_iter()
            .map(|key| {
                assert!(key.ends_with(".log.gz"));

                let obj = get_object(key);
                assert_eq!(obj.content_encoding, Some("gzip".to_string()));

                get_gzipped_lines(obj)
            })
            .flatten()
            .collect::<Vec<_>>();

        assert_eq!(lines, response_lines);
    }

    #[test]
    fn s3_healthchecks() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let healthcheck = S3Sink::healthcheck(&config()).unwrap();
        rt.block_on(healthcheck).unwrap();
    }

    #[test]
    fn s3_healthchecks_invalid_bucket() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let config = S3SinkConfig {
            bucket: "asdflkjadskdaadsfadf".to_string(),
            ..config()
        };
        let healthcheck = S3Sink::healthcheck(&config).unwrap();
        assert_eq!(rt.block_on(healthcheck).unwrap_err(), "Unknown bucket");
    }

    fn client() -> S3Client {
        let region = Region::Custom {
            name: "minio".to_owned(),
            endpoint: "http://localhost:9000".to_owned(),
        };

        S3Sink::create_client(region)
    }

    fn config() -> S3SinkConfig {
        ensure_bucket(&client());

        S3SinkConfig {
            key_prefix: Some(random_string(10) + "/date=%F"),
            bucket: BUCKET.to_string(),
            compression: Compression::None,
            batch_timeout: Some(5),
            region: RegionOrEndpoint::with_endpoint("http://localhost:9000".to_owned()),
            ..Default::default()
        }
    }

    fn ensure_bucket(client: &S3Client) {
        use rusoto_s3::{CreateBucketError, CreateBucketRequest};

        let req = CreateBucketRequest {
            bucket: BUCKET.to_string(),
            ..Default::default()
        };

        let res = client.create_bucket(req);

        match res.sync() {
            Ok(_) | Err(CreateBucketError::BucketAlreadyOwnedByYou(_)) => {}
            Err(e) => match e {
                CreateBucketError::Unknown(b) => {
                    let body = String::from_utf8(b.body.clone()).unwrap();
                    panic!("Couldn't create bucket: {:?}; Body {}", b, body);
                }
                _ => panic!("Couldn't create bucket: {}", e),
            },
        }
    }

    fn get_keys(prefix: String) -> Vec<String> {
        let prefix = prefix.split("/").into_iter().next().unwrap().to_string();

        let list_res = client()
            .list_objects_v2(rusoto_s3::ListObjectsV2Request {
                bucket: BUCKET.to_string(),
                prefix: Some(prefix),
                ..Default::default()
            })
            .sync()
            .unwrap();

        list_res
            .contents
            .unwrap()
            .into_iter()
            .map(|obj| obj.key.unwrap())
            .collect()
    }

    fn get_object(key: String) -> rusoto_s3::GetObjectOutput {
        client()
            .get_object(rusoto_s3::GetObjectRequest {
                bucket: BUCKET.to_string(),
                key: key,
                ..Default::default()
            })
            .sync()
            .unwrap()
    }

    fn get_lines(obj: rusoto_s3::GetObjectOutput) -> Vec<String> {
        let buf_read = BufReader::new(obj.body.unwrap().into_blocking_read());
        buf_read.lines().map(|l| l.unwrap()).collect()
    }

    fn get_gzipped_lines(obj: rusoto_s3::GetObjectOutput) -> Vec<String> {
        let buf_read = BufReader::new(GzDecoder::new(obj.body.unwrap().into_blocking_read()));
        buf_read.lines().map(|l| l.unwrap()).collect()
    }
}
