use crate::record::Record;
use crate::sinks::util::{Buffer, ServiceSink, SinkExt};
use futures::{Async, AsyncSink, Future, Poll, Sink};
use rusoto_core::region::Region;
use rusoto_core::RusotoFuture;
use rusoto_s3::{PutObjectError, PutObjectOutput, PutObjectRequest, S3Client, S3};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tower_in_flight_limit::InFlightLimit;
use tower_service::Service;
use tower_timeout::Timeout;

pub struct S3Sink {
    buffer: Buffer,
    in_flight: Option<RusotoFuture<PutObjectOutput, PutObjectError>>,
    config: S3SinkInnerConfig,
}

pub struct S3SinkInnerConfig {
    pub buffer_size: usize,
    pub key_prefix: String,
    pub bucket: String,
    pub client: S3Client,
    pub gzip: bool,
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
    // TODO: access key and secret token (if the rusoto provider chain stuff isn't good enough)
}

#[typetag::serde(name = "s3")]
impl crate::topology::config::SinkConfig for S3SinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        Ok((new(self.config()?), healthcheck(self.config()?)))
    }
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
        })
    }
}

impl S3Sink {
    fn send_request(&mut self) {
        let body = self.buffer.get_and_reset();
        let response = self.send_body(body);
        assert!(self.in_flight.is_none());
        self.in_flight = Some(response);
    }

    fn send_body(&mut self, body: Vec<u8>) -> RusotoFuture<PutObjectOutput, PutObjectError> {
        // TODO: make this based on the last record in the file
        let filename = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S-%f");
        let extension = if self.config.gzip { ".log.gz" } else { ".log" };

        let request = PutObjectRequest {
            body: Some(body.into()),
            bucket: self.config.bucket.clone(),
            key: format!("{}{}{}", self.config.key_prefix, filename, extension),
            content_encoding: if self.config.gzip {
                Some("gzip".to_string())
            } else {
                None
            },
            ..Default::default()
        };

        self.config.client.put_object(request)
    }

    fn buffer_full(&self) -> bool {
        self.buffer.size() >= self.config.buffer_size
    }

    fn full(&self) -> bool {
        self.buffer_full() && self.in_flight.is_some()
    }
}

impl Sink for S3Sink {
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        if self.full() {
            self.poll_complete()?;

            if self.full() {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        self.buffer.push(&item.line.into_bytes());
        self.buffer.push(b"\n");

        if self.buffer_full() {
            self.poll_complete()?;
        }

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        loop {
            if let Some(ref mut in_flight) = self.in_flight {
                match in_flight.poll() {
                    Err(e) => panic!("{:?}", e),
                    Ok(Async::Ready(_)) => self.in_flight = None,
                    Ok(Async::NotReady) => {
                        if self.buffer_full() {
                            return Ok(Async::NotReady);
                        } else {
                            return Ok(Async::Ready(()));
                        }
                    }
                }
            } else if self.buffer_full() {
                self.send_request();
            } else {
                return Ok(Async::Ready(()));
            }
        }
    }

    fn close(&mut self) -> Result<Async<()>, Self::SinkError> {
        loop {
            if let Some(ref mut in_flight) = self.in_flight {
                match in_flight.poll() {
                    Err(e) => panic!("{:?}", e),
                    Ok(Async::Ready(_)) => self.in_flight = None,
                    Ok(Async::NotReady) => {
                        return Ok(Async::NotReady);
                    }
                }
            } else if !self.buffer.is_empty() {
                self.send_request();
            } else {
                return Ok(Async::Ready(()));
            }
        }
    }
}

impl Service<Buffer> for S3Sink {
    type Response = PutObjectOutput;
    type Error = PutObjectError;
    type Future = RusotoFuture<PutObjectOutput, PutObjectError>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, buf: Buffer) -> Self::Future {
        self.send_body(buf.into())
    }
}

pub fn new(config: S3SinkInnerConfig) -> super::RouterSink {
    let gzip = config.gzip;
    let buffer_size = config.buffer_size;

    let buffer = Buffer::new(config.gzip);

    let inner = S3Sink {
        buffer,
        in_flight: None,
        config,
    };

    let timeout = Timeout::new(inner, Duration::from_secs(10));
    let limited = InFlightLimit::new(timeout, 1);

    let sink = ServiceSink::new(limited)
        .batched(Buffer::new(gzip), buffer_size)
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

#[cfg(test)]
mod tests {
    #![cfg(feature = "s3-integration-tests")]

    use crate::sinks::s3::S3SinkInnerConfig;
    use crate::test_util::{random_lines, random_string};
    use crate::{sinks, Record};
    use flate2::read::GzDecoder;
    use futures::{stream, Sink};
    use rusoto_core::region::Region;
    use rusoto_s3::{S3Client, S3};
    use std::io::{BufRead, BufReader};

    const BUCKET: &str = "router-tests";

    #[test]
    fn s3_insert_message_into() {
        let config = config();
        let prefix = config.key_prefix.clone();
        let sink = sinks::s3::new(config);

        let lines = random_lines(100).take(10).collect::<Vec<_>>();
        let records = lines
            .iter()
            .map(|line| Record::from(line.clone()))
            .collect::<Vec<_>>();

        let pump = sink.send_all(stream::iter_ok(records.into_iter()));

        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let (mut sink, _) = rt.block_on(pump).unwrap();
        rt.block_on(futures::future::poll_fn(move || sink.close()))
            .unwrap();

        let list_res = client()
            .list_objects_v2(rusoto_s3::ListObjectsV2Request {
                bucket: BUCKET.to_string(),
                prefix: Some(prefix),
                ..Default::default()
            })
            .sync()
            .unwrap();

        let keys = list_res
            .contents
            .unwrap()
            .into_iter()
            .map(|obj| obj.key.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(keys.len(), 1);

        let key = keys[0].clone();
        assert!(key.ends_with(".log"));

        let obj = client()
            .get_object(rusoto_s3::GetObjectRequest {
                bucket: BUCKET.to_string(),
                key: key,
                ..Default::default()
            })
            .sync()
            .unwrap();

        assert_eq!(obj.content_encoding, None);

        let response_lines = {
            let buf_read = BufReader::new(obj.body.unwrap().into_blocking_read());
            buf_read.lines().map(|l| l.unwrap()).collect::<Vec<_>>()
        };

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
        let sink = sinks::s3::new(config);

        let lines = random_lines(100).take(30).collect::<Vec<_>>();
        let records = lines
            .iter()
            .map(|line| Record::from(line.clone()))
            .collect::<Vec<_>>();

        let pump = sink.send_all(stream::iter_ok(records.into_iter()));

        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let (mut sink, _) = rt.block_on(pump).unwrap();
        rt.block_on(futures::future::poll_fn(move || sink.close()))
            .unwrap();

        let list_res = client()
            .list_objects_v2(rusoto_s3::ListObjectsV2Request {
                bucket: BUCKET.to_string(),
                prefix: Some(prefix),
                ..Default::default()
            })
            .sync()
            .unwrap();

        let keys = list_res
            .contents
            .unwrap()
            .into_iter()
            .map(|obj| obj.key.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(keys.len(), 3);

        let response_lines = keys
            .into_iter()
            .map(|key| {
                let obj = client()
                    .get_object(rusoto_s3::GetObjectRequest {
                        bucket: BUCKET.to_string(),
                        key: key,
                        ..Default::default()
                    })
                    .sync()
                    .unwrap();

                let response_lines = {
                    let buf_read = BufReader::new(obj.body.unwrap().into_blocking_read());
                    buf_read.lines().map(|l| l.unwrap()).collect::<Vec<_>>()
                };

                response_lines
            })
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
        let sink = sinks::s3::new(config);

        let lines = random_lines(100).take(500).collect::<Vec<_>>();
        let records = lines
            .iter()
            .map(|line| Record::from(line.clone()))
            .collect::<Vec<_>>();

        let pump = sink.send_all(stream::iter_ok(records.into_iter()));

        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let (mut sink, _) = rt.block_on(pump).unwrap();
        rt.block_on(futures::future::poll_fn(move || sink.close()))
            .unwrap();

        let list_res = client()
            .list_objects_v2(rusoto_s3::ListObjectsV2Request {
                bucket: BUCKET.to_string(),
                prefix: Some(prefix),
                ..Default::default()
            })
            .sync()
            .unwrap();

        let keys = list_res
            .contents
            .unwrap()
            .into_iter()
            .map(|obj| obj.key.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(keys.len(), 2);

        let response_lines = keys
            .into_iter()
            .map(|key| {
                assert!(key.ends_with(".log.gz"));

                let obj = client()
                    .get_object(rusoto_s3::GetObjectRequest {
                        bucket: BUCKET.to_string(),
                        key: key,
                        ..Default::default()
                    })
                    .sync()
                    .unwrap();

                assert_eq!(obj.content_encoding, Some("gzip".to_string()));

                let response_lines = {
                    let buf_read =
                        BufReader::new(GzDecoder::new(obj.body.unwrap().into_blocking_read()));
                    buf_read.lines().map(|l| l.unwrap()).collect::<Vec<_>>()
                };

                response_lines
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

}
