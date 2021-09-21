use crate::{config::SinkContext, event::Event, sinks::util::{Compression, SinkBuilder}};
use async_trait::async_trait;
use bytes::Bytes;
use flate2::write::GzEncoder;
use futures::stream::BoxStream;
use std::{
    fmt,
    io::{self, Write},
    num::NonZeroUsize,
    sync::Arc,
    time::Duration,
};
use tower::Service;
use vector_core::{buffers::Acker, event::EventStatus};
use vector_core::{
    event::{EventFinalizers, Finalizable},
    sink::StreamSink,
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
    R: S3RequestBuilder + Send + Sync + 'static,
{
    async fn run(&mut self, input: BoxStream<'static, Event>) -> Result<(), ()> {
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
            .map(Arc::new)
            .expect("same sink should not be run twice");

        let request_builder_rate_limit = NonZeroUsize::new(50);

        let sink = SinkBuilder::new(input)
            .batched(partitioner, self.batch_timeout, self.batch_size_events, self.batch_size_bytes)
            .filter_map(|(key, batch)| async move { key.map(|k| (k, batch)) })
            .concurrent_map(request_builder_rate_limit, move |(key, batch)| {
                let request_builder = Arc::clone(&request_builder);
                async move { request_builder.build_request(key, batch) }
            })
            .driver(service, acker);

        let _ = sink.run().await;
        Ok(())
    }
}

/// Generalized interface for defining how a batch of events will be turned into an S3 request.
pub trait S3RequestBuilder {
    fn compression(&self) -> Compression;

    /// Builds an `S3Request` for the given batch of events, and their partition key.
    fn build_request(&self, key: String, batch: Vec<Event>) -> S3Request;

    /// Encodes an individual event to the provided writer.
    fn encode_event(&self, event: Event, writer: &mut dyn Write) -> io::Result<()>;

    /// Transforms a batch of events into a byte-oriented payload.
    ///
    /// Each event in the batch is run through `encode_event`, and optionally, the entire batch is
    /// compressed based on the value returned by `compression`. The finalizers for all processed
    /// events are merged together and handed back, as well.
    ///
    /// This is a helper method that is expected to be used by `build_request` in most cases.
    ///
    /// TODO: This should likely just be a separate type that can be constructed via builder and
    /// baked into whatever sink needs it, since encoding and compressing events, as well as
    /// collecting their finalizers, is pretty common.  We could additionally make
    /// `S3RequestBuilder` generic over the request output type, and then create a new type for
    /// request building that is essentially this function, but parameterized based on the request
    /// builder that is passed in.  That might be a better way to wrap it all up...
    fn process_event_batch(&self, batch: Vec<Event>) -> (Bytes, EventFinalizers) {
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
            match self.compression() {
                Compression::None => Writer::Plain(buffer),
                Compression::Gzip(level) => Writer::GzipCompressed(GzEncoder::new(buffer, level)),
            }
        };

        let mut finalizers = EventFinalizers::default();

        // Now encode each item into the writer.
        for mut event in batch {
            finalizers.merge(event.take_finalizers());

            let _ = self.encode_event(event, &mut writer)
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
}
