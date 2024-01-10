use std::{error, fmt, path::PathBuf};

use bytes::{Buf, BufMut};
use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
use metrics_util::debugging::DebuggingRecorder;
use metrics_util::layers::Layer;
use tracing::Span;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use vector_buffers::{
    encoding::FixedEncodable,
    topology::{
        builder::TopologyBuilder,
        channel::{BufferReceiver, BufferSender},
    },
    BufferType, EventCount,
};
use vector_common::byte_size_of::ByteSizeOf;
use vector_common::finalization::{AddBatchNotifier, BatchNotifier, EventFinalizers, Finalizable};

#[derive(Clone, Copy, Debug)]
pub struct Message<const N: usize> {
    id: u64,
    _padding: [u64; N],
}

impl<const N: usize> Message<N> {
    fn new(id: u64) -> Self {
        Message {
            id,
            _padding: [0; N],
        }
    }
}

impl<const N: usize> AddBatchNotifier for Message<N> {
    fn add_batch_notifier(&mut self, batch: BatchNotifier) {
        drop(batch); // Incorrect but fast
    }
}

impl<const N: usize> ByteSizeOf for Message<N> {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

impl<const N: usize> EventCount for Message<N> {
    fn event_count(&self) -> usize {
        1
    }
}

impl<const N: usize> Finalizable for Message<N> {
    fn take_finalizers(&mut self) -> EventFinalizers {
        Default::default() // This benchmark doesn't need finalization
    }
}

#[derive(Debug)]
pub struct EncodeError;

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for EncodeError {}

#[derive(Debug)]
pub struct DecodeError;

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for DecodeError {}

impl<const N: usize> FixedEncodable for Message<N> {
    type EncodeError = EncodeError;
    type DecodeError = DecodeError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::EncodeError>
    where
        B: BufMut,
        Self: Sized,
    {
        buffer.put_u64(self.id);
        for _ in 0..N {
            // this covers self._padding
            buffer.put_u64(0);
        }
        Ok(())
    }

    fn decode<B>(mut buffer: B) -> Result<Self, Self::DecodeError>
    where
        B: Buf,
        Self: Sized,
    {
        let id = buffer.get_u64();
        for _ in 0..N {
            // this covers self._padding
            _ = buffer.get_u64();
        }
        Ok(Message::new(id))
    }
}

#[allow(dead_code)]
#[allow(clippy::type_complexity)]
pub async fn setup<const N: usize>(
    variant: BufferType,
    total_events: usize,
    data_dir: Option<PathBuf>,
    id: String,
) -> (
    BufferSender<Message<N>>,
    BufferReceiver<Message<N>>,
    Vec<Message<N>>,
) {
    let mut messages: Vec<Message<N>> = Vec::with_capacity(total_events);
    for i in 0..total_events {
        messages.push(Message::new(i as u64));
    }

    let mut builder = TopologyBuilder::default();
    variant
        .add_to_builder(&mut builder, data_dir, id)
        .expect("should not fail to add variant to builder");
    let (tx, rx) = builder
        .build(String::from("benches"), Span::none())
        .await
        .expect("should not fail to build topology");

    (tx, rx, messages)
}

pub fn init_instrumentation() {
    if metrics::try_recorder().is_none() {
        let subscriber = tracing_subscriber::Registry::default().with(MetricsLayer::new());
        tracing::subscriber::set_global_default(subscriber).unwrap();

        let recorder = TracingContextLayer::all().layer(DebuggingRecorder::new());
        metrics::set_boxed_recorder(Box::new(recorder)).unwrap();
    }
}

//
// Measurements
//
// The nature of our buffer is such that the underlying representation is hidden
// behind an abstract interface. As a happy consequence of this our benchmark
// measurements are common. "Write Then Read" writes all messages into the
// buffer and then reads them out. "Write And Read" writes a message and then
// reads it from the buffer.
//

pub async fn wtr_measurement<const N: usize>(
    mut sender: BufferSender<Message<N>>,
    mut receiver: BufferReceiver<Message<N>>,
    messages: Vec<Message<N>>,
) {
    for msg in messages.into_iter() {
        sender.send(msg, None).await.unwrap();
    }
    drop(sender);

    while receiver.next().await.is_some() {}
}

pub async fn war_measurement<const N: usize>(
    mut sender: BufferSender<Message<N>>,
    mut receiver: BufferReceiver<Message<N>>,
    messages: Vec<Message<N>>,
) {
    for msg in messages.into_iter() {
        sender.send(msg, None).await.unwrap();
        _ = receiver.next().await.unwrap();
    }
}
