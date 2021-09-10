use crate::sinks::util::sink::ServiceLogic;
use crate::{
    config::{log_schema, SinkContext},
    event::Event,
    sinks::util::{
        buffer::GZIP_FAST, encoding::EncodingConfiguration, sink::StdServiceLogic, Compression,
    },
};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use flate2::write::GzEncoder;
use futures::{
    stream::{BoxStream, FuturesUnordered, StreamExt},
    FutureExt, TryFutureExt,
};
use std::{
    collections::HashMap,
    convert::TryInto,
    fmt::Debug,
    io::{self, Write},
    num::NonZeroUsize,
    time::Duration,
};
use tokio::{
    pin, select,
    sync::{
        mpsc::{channel, Receiver},
        oneshot,
    },
};
use tower::{Service, ServiceExt};
use tracing_futures::Instrument;
use uuid::Uuid;
use vector_core::{
    buffers::Acker,
    event::{EventFinalizers, Finalizable},
    sink::StreamSink,
    stream::batcher::Batcher,
};

use crate::sinks::s3_common::partitioner::KeyPartitioner;
use crate::sinks::s3_common::service::S3Request;
use crate::sinks::util::encoding::EncodingConfig;
use crate::sinks::util::sink::Response;

pub struct S3Sink<S, R>
where
    R: S3RequestBuilder,
{
    acker: Option<Acker>,
    service: Option<S>,
    request_builder: R,
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
            request_builder,
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
    S::Response: Response + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
    R: S3RequestBuilder + Send,
{
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // All sinks do the same fundamental job: take in events, and ship them
        // out. Empirical testing shows that our number one priority for high
        // throughput needs to be servicing I/O as soon as we possibly can.  In
        // order to do that, we'll spin up a separate task that deals
        // exclusively with I/O, while we deal with everything else here:
        // batching, ordering, and so on.
        let (io_tx, io_rx) = channel(64);
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

        let io = run_io(io_rx, service, acker).in_current_span();
        let _ = tokio::spawn(io);

        let batcher = Batcher::new(
            input,
            partitioner,
            self.batch_timeout,
            self.batch_size_events,
            self.batch_size_bytes,
        );
        pin!(batcher);

        while let Some((key, batch)) = batcher.next().await {
            match key {
                Some(key) => {
                    // We could push this down to the I/O task if we wanted to.
                    let request = self.request_builder.build_request(key, batch);
                    if io_tx.send(request).await.is_err() {
                        error!(
                            "Sink I/O channel should not be closed before sink itself is closed."
                        );
                        return Err(());
                    }
                }
                // Partitioning failed for one or more events.
                //
                // TODO: this might be where we would insert something like the
                // proposed error handling/dead letter queue stuff; events that
                // _we_ can't handle, but some other component may be able to
                // salvage
                None => {
                    continue;
                }
            }
        }

        Ok(())
    }
}

async fn run_io<S>(mut rx: Receiver<S3Request>, mut service: S, acker: Acker)
where
    S: Service<S3Request>,
    S::Future: Send + 'static,
    S::Response: Response + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    let in_flight = FuturesUnordered::new();
    let mut pending_acks = HashMap::new();
    let mut seq_head: u64 = 0;
    let mut seq_tail: u64 = 0;

    pin!(in_flight);

    loop {
        select! {
            Some(req) = rx.recv() => {
                // Rebind the variable to avoid a bug with the pattern matching
                // in `select!`: https://github.com/tokio-rs/tokio/issues/4076
                let mut req = req;
                let seqno = seq_head;
                seq_head += 1;

                let (tx, rx) = oneshot::channel();

                in_flight.push(rx);

                trace!(
                    message = "Submitting service request.",
                    in_flight_requests = in_flight.len()
                );
                // TODO: This likely need be parameterized, which builds a
                // stronger case for following through with the comment
                // mentioned below.
                let logic = StdServiceLogic::default();
                // TODO: I'm not entirely happy with how we're smuggling
                // batch_size/finalizers this far through, from the finished
                // batch all the way through to the concrete request type...we
                // lifted this code from `ServiceSink` directly, but we should
                // probably treat it like `PartitionBatcher` and shove it into a
                // single, encapsulated type instead.
                let batch_size = req.batch_size;
                let finalizers = req.take_finalizers();

                let svc = service.ready().await.expect("should not get error when waiting for svc readiness");
                let fut = svc.call(req)
                    .err_into()
                    .map(move |result| {
                        logic.update_finalizers(result, finalizers);

                        // If the rx end is dropped we still completed
                        // the request so this is a weird case that we can
                        // ignore for now.
                        let _ = tx.send((seqno, batch_size));
                    })
                    .instrument(info_span!("request", request_id = %seqno));
                tokio::spawn(fut);
            },

            Some(Ok((seqno, batch_size))) = in_flight.next() => {
                trace!("pending batch {} finished (n={})", seqno, batch_size);
                pending_acks.insert(seqno, batch_size);

                let mut num_to_ack = 0;
                while let Some(ack_size) = pending_acks.remove(&seq_tail) {
                    num_to_ack += ack_size;
                    seq_tail += 1
                }
                trace!(message = "Acking events.", acking_num = num_to_ack);
                acker.ack(num_to_ack);
            },

            else => break
        }
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
