use crate::buffers::Acker;
use crate::record::Record;
use crate::sinks::util::{BatchServiceSink, Buffer, SinkExt};
use futures::{Future, Poll, Sink};
use rusoto_core::region::Region;
use rusoto_core::RusotoFuture;
use rusoto_s3::{PutObjectError, PutObjectOutput, PutObjectRequest, S3Client, S3};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_trace_futures::{Instrument, Instrumented};
use tower::{Service, ServiceBuilder};

pub struct S3Sink {
    config: S3SinkInnerConfig,
}

pub struct S3SinkInnerConfig {
    pub buffer_size: usize,
    pub key_prefix: String,
    pub bucket: String,
    pub client: S3Client,
    pub gzip: bool,
    pub max_linger_secs: u64,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct S3SinkConfig {
    pub bucket: String,
    pub key_prefix: String,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub buffer_size: usize,
    pub gzip: bool,
    pub max_linger_secs: Option<u64>,
    // TODO: access key and secret token (if the rusoto provider chain stuff isn't good enough)
}

#[typetag::serde(name = "s3")]
impl crate::topology::config::SinkConfig for S3SinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        Ok((new(self.config()?, acker), healthcheck(self.config()?)))
    }
}

pub fn new(config: S3SinkInnerConfig, acker: Acker) -> super::RouterSink {
    let gzip = config.gzip;
    let buffer_size = config.buffer_size;
    let max_linger_secs = config.max_linger_secs;

    let s3 = S3Sink { config };

    let svc = ServiceBuilder::new()
        .in_flight_limit(1)
        .timeout(Duration::from_secs(10))
        .service(s3)
        .expect("This is a bug, no spawnning");

    let sink = BatchServiceSink::new(svc, acker)
        .batched_with_min(
            Buffer::new(gzip),
            buffer_size,
            Duration::from_secs(max_linger_secs),
        )
        .with(|record: Record| {
            let mut bytes: Vec<u8> = record.into();
            bytes.push(b'\n');
            Ok(bytes)
        });

    Box::new(sink)
}

pub fn healthcheck(config: S3SinkInnerConfig) -> super::Healthcheck {
    use rusoto_s3::{HeadBucketError, HeadBucketRequest};

    let request = HeadBucketRequest {
        bucket: config.bucket,
    };

    let response = config.client.head_bucket(request);

    let healthcheck = response.map_err(|err| match err {
        HeadBucketError::Unknown(response) => match response.status {
            http::status::StatusCode::FORBIDDEN => "Invalid credentials".to_string(),
            http::status::StatusCode::NOT_FOUND => "Unknown bucket".to_string(),
            status => format!("Unknown error: Status code: {}", status),
        },
        err => err.to_string(),
    });

    Box::new(healthcheck)
}

impl S3SinkConfig {
    fn region(&self) -> Result<Region, String> {
        if self.region.is_some() && self.endpoint.is_some() {
            Err("Only one of 'region' or 'endpoint' can be specified".to_string())
        } else if let Some(region) = &self.region {
            region.parse::<Region>().map_err(|e| e.to_string())
        } else if let Some(endpoint) = &self.endpoint {
            Ok(Region::Custom {
                name: "custom".to_owned(),
                endpoint: endpoint.clone(),
            })
        } else {
            Err("Must set 'region' or 'endpoint'".to_string())
        }
    }

    fn config(&self) -> Result<S3SinkInnerConfig, String> {
        let region = self.region()?;

        Ok(S3SinkInnerConfig {
            client: rusoto_s3::S3Client::new(region),
            gzip: self.gzip,
            buffer_size: self.buffer_size,
            key_prefix: self.key_prefix.clone(),
            bucket: self.bucket.clone(),
            max_linger_secs: self.max_linger_secs.unwrap_or(300),
        })
    }
}

impl S3Sink {
    fn send_body(&mut self, body: Vec<u8>) -> RusotoFuture<PutObjectOutput, PutObjectError> {
        // TODO: make this based on the last record in the file
        let filename = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S-%f");
        let extension = if self.config.gzip { ".log.gz" } else { ".log" };
        let key = format!("{}{}{}", self.config.key_prefix, filename, extension);

        info!(
            { s3_sink_bytes_flushed_counter = body.len() },
            "Flushing {} to S3 ({} bytes)",
            key,
            body.len()
        );

        let request = PutObjectRequest {
            body: Some(body.into()),
            bucket: self.config.bucket.clone(),
            key,
            content_encoding: if self.config.gzip {
                Some("gzip".to_string())
            } else {
                None
            },
            ..Default::default()
        };

        self.config.client.put_object(request)
    }
}

impl Service<Vec<u8>> for S3Sink {
    type Response = PutObjectOutput;
    type Error = PutObjectError;
    type Future = Instrumented<RusotoFuture<PutObjectOutput, PutObjectError>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, body: Vec<u8>) -> Self::Future {
        self.send_body(body).instrument(info_span!("s3_request"))
    }
}

#[cfg(test)]
mod tests {
    #![cfg(feature = "s3-integration-tests")]

    use crate::buffers::Acker;
    use crate::{
        sinks::{self, s3::S3SinkInnerConfig},
        test_util::{block_on, random_lines_with_stream, random_string},
        Record,
    };
    use flate2::read::GzDecoder;
    use futures::{Future, Sink};
    use rusoto_core::region::Region;
    use rusoto_s3::{S3Client, S3};
    use std::io::{BufRead, BufReader};

    const BUCKET: &str = "router-tests";

    #[test]
    fn s3_insert_message_into() {
        let config = config();
        let prefix = config.key_prefix.clone();
        let sink = sinks::s3::new(config, Acker::Null);

        let (lines, records) = random_lines_with_stream(100, 10);

        let pump = sink.send_all(records);
        block_on(pump).unwrap();

        let keys = get_keys(prefix);
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

        let config = S3SinkInnerConfig {
            buffer_size: 1000,
            ..config()
        };
        let prefix = config.key_prefix.clone();
        let sink = sinks::s3::new(config, Acker::Null);

        let (lines, records) = random_lines_with_stream(100, 30);

        let pump = sink.send_all(records);
        block_on(pump).unwrap();

        let keys = get_keys(prefix);
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

        let config = S3SinkInnerConfig {
            buffer_size: 1000,
            ..config()
        };
        let prefix = config.key_prefix.clone();
        let sink = sinks::s3::new(config, Acker::Null);

        let (lines, _) = random_lines_with_stream(100, 30);

        let (tx, rx) = futures::sync::mpsc::channel(1);
        let pump = sink.send_all(rx).map(|_| ()).map_err(|_| ());

        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(pump);

        let mut tx = tx.wait();
        for line in lines.iter().take(15) {
            tx.send(Record::from(line.as_str())).unwrap();
        }

        std::thread::sleep(std::time::Duration::from_millis(100));

        for line in lines.iter().skip(15) {
            tx.send(Record::from(line.as_str())).unwrap();
        }
        drop(tx);

        crate::test_util::shutdown_on_idle(rt);

        let keys = get_keys(prefix);
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

        let config = S3SinkInnerConfig {
            buffer_size: 1000,
            gzip: true,
            ..config()
        };
        let prefix = config.key_prefix.clone();
        let sink = sinks::s3::new(config, Acker::Null);

        let (lines, records) = random_lines_with_stream(100, 500);

        let pump = sink.send_all(records);
        block_on(pump).unwrap();

        let keys = get_keys(prefix);
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

        // OK
        {
            let healthcheck = sinks::s3::healthcheck(config());
            rt.block_on(healthcheck).unwrap();
        }

        // Bad credentials
        {
            let credentials = rusoto_credential::StaticProvider::new_minimal(
                "asdf".to_string(),
                "1234".to_string(),
            );

            let dispatcher = rusoto_core::request::HttpClient::new().unwrap();

            let region = Region::Custom {
                name: "minio".to_owned(),
                endpoint: "http://localhost:9000".to_owned(),
            };

            let client = S3Client::new_with(dispatcher, credentials, region);

            let config = S3SinkInnerConfig { client, ..config() };
            let healthcheck = sinks::s3::healthcheck(config);
            assert_eq!(rt.block_on(healthcheck).unwrap_err(), "Invalid credentials")
        }

        // Inaccessible bucket
        {
            let config = S3SinkInnerConfig {
                bucket: "asdflkjadskdaadsfadf".to_string(),
                ..config()
            };
            let healthcheck = sinks::s3::healthcheck(config);
            assert_eq!(rt.block_on(healthcheck).unwrap_err(), "Unknown bucket");
        }
    }

    fn client() -> S3Client {
        let region = Region::Custom {
            name: "localstack".to_owned(),
            endpoint: "http://localhost:9000".to_owned(),
        };

        let static_creds = rusoto_core::credential::StaticProvider::new(
            "test-access-key".into(),
            "test-secret-key".into(),
            None,
            None,
        );

        let client = rusoto_core::HttpClient::new().unwrap();

        S3Client::new_with(client, static_creds, region)
    }

    fn config() -> S3SinkInnerConfig {
        ensure_bucket(&client());

        S3SinkInnerConfig {
            client: client(),
            key_prefix: random_string(10) + "/",
            buffer_size: 2 * 1024 * 1024,
            bucket: BUCKET.to_string(),
            gzip: false,
            max_linger_secs: 5,
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
            e => {
                panic!("Couldn't create bucket: {:?}", e);
            }
        }
    }

    fn get_keys(prefix: String) -> Vec<String> {
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
