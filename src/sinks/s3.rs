use crate::record::Record;
use futures::{future, Async, AsyncSink, Future, Sink};
use rusoto_core::RusotoFuture;
use rusoto_s3::{PutObjectError, PutObjectOutput, PutObjectRequest, S3Client, S3};
use crate::sinks::util::size_buffered::Buffer;

pub struct S3Sink {
    buffer: Buffer,
    in_flight: Option<RusotoFuture<PutObjectOutput, PutObjectError>>,
    config: S3SinkConfig,
}

pub struct S3SinkConfig {
    pub buffer_size: usize,
    pub key_prefix: String,
    pub bucket: String,
    pub client: S3Client,
    pub gzip: bool,
}

impl S3Sink {
    fn send_request(&mut self) {
        let body = self.buffer.get_and_reset();

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

        let response = self.config.client.put_object(request);
        assert!(self.in_flight.is_none());
        self.in_flight = Some(response);
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

pub fn new(config: S3SinkConfig) -> super::RouterSinkFuture {
    let buffer = Buffer::new(config.gzip);

    let sink = S3Sink {
        buffer,
        in_flight: None,
        config,
    };

    let sink: super::RouterSink = Box::new(sink);
    Box::new(future::ok(sink))
}
