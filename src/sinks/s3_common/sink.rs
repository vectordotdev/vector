use crate::{
    config::SinkContext,
    event::Event,
    sinks::util::{buffer::GZIP_FAST, Compression},
};
use async_trait::async_trait;
use bytes::Bytes;
use flate2::write::GzEncoder;
use futures::{
    stream::{BoxStream, StreamExt},
    FutureExt,
};
use std::{
    fmt,
    io::{self, Write},
    num::NonZeroUsize,
    time::Duration,
};
use tower::Service;
use vector_core::stream::driver::Driver;
use vector_core::{buffers::Acker, event::EventStatus};
use vector_core::{
    event::{EventFinalizers, Finalizable},
    sink::StreamSink,
    stream::batcher::Batcher,
};

use crate::sinks::s3_common::partitioner::KeyPartitioner;
use crate::sinks::s3_common::service::S3Request;

pub struct S3Sink<S, R>
where
    R: S3RequestBuilder,
{
    acker: Option<Acker>,
    service: Option<S>,
    request_builder: Option<R>,
    partitioner: Option<KeyPartitioner>,
    batch_size_bytes: Option<NonZeroUsize>,
    batch_size_events: NonZeroUsize,
    batch_timeout: Duration,
}

impl<S, R> S3Sink<S, R>
where
    R: S3RequestBuilder,
{
    pub fn new(
        cx: SinkContext,
        service: S,
        request_builder: R,
        partitioner: KeyPartitioner,
        batch_size_bytes: Option<NonZeroUsize>,
        batch_size_events: NonZeroUsize,
        batch_timeout: Duration,
    ) -> Self {
        Self {
            acker: Some(cx.acker()),
            service: Some(service),
            request_builder: Some(request_builder),
            partitioner: Some(partitioner),
            batch_size_bytes,
            batch_size_events,
            batch_timeout,
        }
    }
}

#[async_trait]
impl<S, R> StreamSink for S3Sink<S, R>
where
    S: Service<S3Request> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: AsRef<EventStatus> + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
    R: S3RequestBuilder + Send,
{
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // All sinks do the same fundamental job: take in events, and ship them
        // out. Empirical testing shows that our number one priority for high
        // throughput needs to be servicing I/O as soon as we possibly can.  In
        // order to do that, we'll spin up a separate task that deals
        // exclusively with I/O, while we deal with everything else here:
        // batching, ordering, and so on.
        let service = self
            .service
            .take()
            .expect("same sink should not be run twice");
        let acker = self
            .acker
            .take()
            .expect("same sink should not be run twice");
        let partitioner = self
            .partitioner
            .take()
            .expect("same sink should not be run twice");
        let request_builder = self
            .request_builder
            .take()
            .expect("same sink should not be run twice");

        let batcher = Batcher::new(
            input,
            partitioner,
            self.batch_timeout,
            self.batch_size_events,
            self.batch_size_bytes,
        );

        let processed_batches = batcher.filter_map(|(key, batch)| async move {
            key.map(|key| request_builder.build_request(key, batch))
        });

        let driver = Driver::new(processed_batches, service, acker);
        driver.run().await
    }
}

pub trait S3RequestBuilder {
    fn build_request(&mut self, key: String, batch: Vec<Event>) -> S3Request;
}

pub trait S3EventEncoding {
    fn encode_event(&mut self, event: Event, writer: &mut dyn Write) -> io::Result<()>;
}

pub fn process_event_batch<E: S3EventEncoding>(
    batch: Vec<Event>,
    encoding: &mut E,
    compression: Compression,
) -> (Bytes, EventFinalizers) {
    enum Writer {
        Plain(Vec<u8>),
        GzipCompressed(GzEncoder<Vec<u8>>),
    }

    impl Write for Writer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self {
                Writer::Plain(inner_buf) => inner_buf.write(buf),
                Writer::GzipCompressed(writer) => writer.write(buf),
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            match self {
                Writer::Plain(_) => Ok(()),
                Writer::GzipCompressed(writer) => writer.flush(),
            }
        }
    }

    // Build our compressor first, so that we can encode directly into it.
    let mut writer = {
        // This is a best guess, because encoding could add a good chunk of
        // overhead to the raw, in-memory representation of an event, but if
        // we're compressing, then we should end up net below the capacity.
        let buffer = Vec::with_capacity(1_024);
        match compression {
            Compression::None => Writer::Plain(buffer),
            Compression::Gzip(level) => {
                let level = level.unwrap_or(GZIP_FAST);
                Writer::GzipCompressed(GzEncoder::new(
                    buffer,
                    flate2::Compression::new(level as u32),
                ))
            }
        }
    };

    let mut finalizers = EventFinalizers::default();

    // Now encode each item into the writer.
    for mut event in batch {
        finalizers.merge(event.take_finalizers());

        let _ = encoding
            .encode_event(event, &mut writer)
            .expect("failed to encode event into writer; this is a bug!");
    }

    // Extract the buffer and push it back in a frozen state.
    let buf = match writer {
        Writer::Plain(buf) => buf.into(),
        Writer::GzipCompressed(writer) => writer
            .finish()
            .expect("gzip writer should not fail to finish")
            .into(),
    };

    (buf, finalizers)
}
