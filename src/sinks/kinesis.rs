use super::Record;
use futures::{Async, AsyncSink, Future, Poll, Sink, StartSend};
use rand::random;
use rusoto_core::Region;
use rusoto_kinesis::{Kinesis, KinesisClient, PutRecordsInput, PutRecordsRequestEntry};
use serde::{Deserialize, Serialize};
use std::fmt;
use tower_service::Service;

pub struct KinesisService {
    client: KinesisClient,
    config: KinesisSinkConfig,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct KinesisSinkConfig {
    pub stream_name: String,
    pub region: String,
    pub buffer_size: usize,
}

impl KinesisService {
    pub fn new(config: KinesisSinkConfig) -> impl Sink<SinkItem = Record, SinkError = ()> {
        let region = config.region.clone().parse::<Region>().unwrap();
        let client = KinesisClient::new(region);

        let buffer = VecBuffer::new(config.buffer_size);
        let service = KinesisService { client, config };

        // TODO: construct service middleware here

        BufferSink::new(buffer, service)
    }

    fn gen_partition_key(&mut self) -> String {
        random::<[char; 16]>()
            .into_iter()
            .fold(String::new(), |mut s, c| {
                s.push(*c);
                s
            })
    }
}

impl Service<Vec<Vec<u8>>> for KinesisService {
    type Response = ();
    type Error = ();
    type Future = Box<Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, items: Vec<Vec<u8>>) -> Self::Future {
        let records = items
            .into_iter()
            .map(|data| PutRecordsRequestEntry {
                data,
                partition_key: self.gen_partition_key(),
                ..Default::default()
            })
            .collect();

        let request = PutRecordsInput {
            records,
            stream_name: self.config.stream_name.clone(),
        };

        let fut = self
            .client
            .put_records(request)
            .map(|_| ())
            .map_err(|e| panic!("Kinesis Error: {:?}", e));

        Box::new(fut)
    }
}

// === impl Buffersink ===

pub struct BufferSink<S, B>
where
    B: Buffer,
    S: Service<Vec<B::Item>>,
{
    buffer: B,
    service: S,
    state: State<S::Future>,
}

enum State<T> {
    Poll(T),
    Buffering,
}

impl<S, B> BufferSink<S, B>
where
    B: Buffer,
    S: Service<Vec<B::Item>>,
{
    pub fn new(buffer: B, service: S) -> Self {
        BufferSink {
            buffer,
            service,
            state: State::Buffering,
        }
    }
}

impl<S, B> Sink for BufferSink<S, B>
where
    B: Buffer,
    B::Item: From<Record>,
    S: Service<Vec<B::Item>>,
    S::Error: fmt::Debug,
{
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.buffer.full() {
            self.poll_complete()?;

            if self.buffer.full() {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        self.buffer.push(item.into());

        if self.buffer.full() {
            self.poll_complete()?;
        }

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        loop {
            match self.state {
                State::Poll(ref mut fut) => match fut.poll() {
                    Ok(Async::Ready(_)) => {
                        self.state = State::Buffering;
                        continue;
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(err) => panic!("Error sending request: {:?}", err),
                },

                State::Buffering => {
                    if self.buffer.full() {
                        let items = self.buffer.flush();
                        let fut = self.service.call(items);
                        self.state = State::Poll(fut);

                        continue;
                    } else {
                        // check timer here???
                        // Buffer isnt full and there isn't an inflight request
                        if !self.buffer.empty() {
                            // Buffer isnt empty, isnt full and there is no inflight
                            // so lets take the rest of the buffer and send it.
                            let items = self.buffer.flush();
                            let fut = self.service.call(items);
                            self.state = State::Poll(fut);

                            continue;
                        } else {
                            return Ok(Async::Ready(()));
                        }
                    }
                }
            }
        }
    }
}

pub trait Buffer {
    type Item;

    fn push(&mut self, item: Self::Item);

    fn flush(&mut self) -> Vec<Self::Item>;

    fn full(&self) -> bool;

    fn empty(&self) -> bool;
}

pub struct VecBuffer<T> {
    inner: Vec<T>,
    size: usize,
}

impl<T> VecBuffer<T> {
    pub fn new(size: usize) -> Self {
        VecBuffer {
            inner: Vec::new(),
            size,
        }
    }
}

impl<T> Buffer for VecBuffer<T> {
    type Item = T;

    fn full(&self) -> bool {
        self.inner.len() >= self.size
    }

    fn push(&mut self, item: T) {
        self.inner.push(item);
    }

    fn flush(&mut self) -> Vec<T> {
        // TODO(lucio): make this unsafe replace?
        self.inner.drain(..).collect()
    }

    fn empty(&self) -> bool {
        self.inner.is_empty()
    }
}
