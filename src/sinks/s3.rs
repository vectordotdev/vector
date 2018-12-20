use crate::record::Record;
use flate2::write::GzEncoder;
use futures::{future, Async, AsyncSink, Future, Sink};
use rusoto_core::RusotoFuture;
use rusoto_s3::{PutObjectError, PutObjectOutput, PutObjectRequest, S3Client, S3};
use std::io::Write;
use std::mem;

pub struct S3Sink {
    client: S3Client,
    buffer: Buffer,
    cutoff_size: usize,
    in_flight: Option<RusotoFuture<PutObjectOutput, PutObjectError>>,
    prefix: String,
    bucket: String,
}

impl S3Sink {
    fn send_request(&mut self) {
        let new_buffer = Buffer::fresh(self.buffer.is_gzip());
        let body = mem::replace(&mut self.buffer, new_buffer);
        let body = body.finalize();

        // TODO: make this based on the last record in the file
        let filename = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S-%f");
        let extension = if self.buffer.is_gzip() {
            ".log.gz"
        } else {
            ".log"
        };

        let request = PutObjectRequest {
            body: Some(body.into()),
            bucket: self.bucket.clone(),
            key: format!("{}{}{}", self.prefix, filename, extension),
            content_encoding: if self.buffer.is_gzip() {
                Some("gzip".to_string())
            } else {
                None
            },
            ..Default::default()
        };

        let response = self.client.put_object(request);
        assert!(self.in_flight.is_none());
        self.in_flight = Some(response);
    }

    fn full(&self) -> bool {
        self.buffer.size() >= self.cutoff_size && self.in_flight.is_some()
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

        if self.buffer.size() >= self.cutoff_size {
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
                        if self.buffer.size() < self.cutoff_size {
                            return Ok(Async::Ready(()));
                        } else {
                            return Ok(Async::NotReady);
                        }
                    }
                }
            } else if self.buffer.size() >= self.cutoff_size {
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

pub fn new(
    client: S3Client,
    prefix: String,
    cutoff_size: usize,
    bucket: String,
    gzip: bool,
) -> super::RouterSinkFuture {
    let buffer = Buffer::fresh(gzip);

    let sink = S3Sink {
        client,
        buffer,
        cutoff_size,
        in_flight: None,
        prefix,
        bucket,
    };

    let sink: super::RouterSink = Box::new(sink);
    Box::new(future::ok(sink))
}

enum Buffer {
    Plain(Vec<u8>),
    Gzip(GzEncoder<Vec<u8>>),
}

impl Buffer {
    fn fresh(gzip: bool) -> Self {
        if gzip {
            Buffer::Gzip(GzEncoder::new(Vec::new(), flate2::Compression::default()))
        } else {
            Buffer::Plain(Vec::new())
        }
    }

    fn is_gzip(&self) -> bool {
        match self {
            Buffer::Plain(_) => false,
            Buffer::Gzip(_) => true,
        }
    }

    fn finalize(self) -> Vec<u8> {
        match self {
            Buffer::Plain(inner) => inner,
            Buffer::Gzip(inner) => inner
                .finish()
                .expect("This can't fail because the inner writer is a Vec"),
        }
    }

    fn push(&mut self, input: &[u8]) {
        match self {
            Buffer::Plain(inner) => {
                inner.extend_from_slice(input);
            }
            Buffer::Gzip(inner) => {
                inner.write_all(input).unwrap();
            }
        }
    }

    fn size(&self) -> usize {
        match self {
            Buffer::Plain(inner) => inner.len(),
            Buffer::Gzip(inner) => inner.get_ref().len(),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Buffer::Plain(inner) => inner.is_empty(),
            Buffer::Gzip(inner) => inner.get_ref().is_empty(),
        }
    }
}
