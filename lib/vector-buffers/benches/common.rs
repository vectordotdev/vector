use std::{
    error, fmt,
    num::{NonZeroU64, NonZeroUsize},
    path::PathBuf,
};

use bytes::{Buf, BufMut};
use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
use metrics_util::{debugging::DebuggingRecorder, layers::Layer};
use tracing::Span;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use vector_buffers::{
    BufferType, Bufferable, EventCount, MemoryBufferSize, WhenFull,
    encoding::FixedEncodable,
    topology::{
        builder::TopologyBuilder,
        channel::{BufferReceiver, BufferSender},
    },
};
use vector_common::{
    byte_size_of::ByteSizeOf,
    finalization::{
        AddBatchNotifier, BatchNotifier, EventFinalizers, Finalizable, MergeFinalizable,
    },
};

/// Owns a temporary benchmark data directory and creates isolated iteration paths.
pub struct DataDir {
    index: usize,
    base: PathBuf,
    current: Option<PathBuf>,
}

impl DataDir {
    pub fn new(name: &str) -> Self {
        let mut base = std::env::temp_dir();
        base.push(name);
        if base.exists() {
            // Remove data abandoned by an interrupted benchmark before reusing its deterministic path.
            std::fs::remove_dir_all(&base).expect("could not remove stale base dir");
        }
        std::fs::create_dir_all(&base).expect("could not make base dir");

        Self {
            index: 0,
            base,
            current: None,
        }
    }

    /// Removes the prior iteration directory before creating the next one.
    ///
    /// This cleanup intentionally occurs in Criterion's setup closure, outside the timed routine.
    /// Disk buffers must allocate at least 256 MiB, so retaining a directory per iteration would
    /// quickly exhaust disk space; deleting it from the timed routine would benchmark cleanup.
    pub fn next(&mut self) -> PathBuf {
        if let Some(current) = self.current.take() {
            std::fs::remove_dir_all(current).expect("could not remove previous dir");
        }

        let mut next = self.base.clone();
        next.push(self.index.to_string());
        self.index += 1;
        std::fs::create_dir_all(&next).expect("could not make next dir");

        self.current = Some(next.clone());
        next
    }
}

impl Drop for DataDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.base).expect("could not remove base dir");
    }
}

pub fn disk_buffer(max_size: u64) -> BufferType {
    BufferType::DiskV2 {
        max_size: NonZeroU64::new(max_size).expect("disk capacity must be non-zero"),
        when_full: WhenFull::DropNewest,
    }
}

pub fn memory_buffer_by_events(max_events: usize) -> BufferType {
    BufferType::Memory {
        size: MemoryBufferSize::MaxEvents(
            NonZeroUsize::new(max_events).expect("event count must be non-zero"),
        ),
        when_full: WhenFull::DropNewest,
    }
}

pub fn memory_buffer_by_bytes(max_size: usize) -> BufferType {
    BufferType::Memory {
        size: MemoryBufferSize::MaxSize(
            NonZeroUsize::new(max_size).expect("memory capacity must be non-zero"),
        ),
        when_full: WhenFull::DropNewest,
    }
}

#[derive(Clone, Debug)]
pub struct Message<const N: usize> {
    id: u64,
    // Purpose of `_heap_allocated` is to simulate memory pressure in the buffer benchmarks when the
    // max_size option is selected.
    _heap_allocated: Box<[u64; N]>,
    _padding: [u64; N],
}

impl<const N: usize> Message<N> {
    fn new(id: u64) -> Self {
        Message {
            id,
            _heap_allocated: Box::new([0; N]),
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
        N * std::mem::size_of::<u64>()
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

impl<const N: usize> MergeFinalizable for Message<N> {
    fn merge_finalizers(&mut self, _finalizers: EventFinalizers) {
        // This benchmark doesn't need finalization.
    }
}

#[derive(Debug)]
pub struct EncodeError;

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl error::Error for EncodeError {}

#[derive(Debug)]
pub struct DecodeError;

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
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
        for _ in 0..(N * 2) {
            // this covers self._padding and self.heap_allocated
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
        for _ in 0..(N * 2) {
            // this covers self._padding and self.heap_allocated
            _ = buffer.get_u64();
        }
        Ok(Message::new(id))
    }
}

pub struct BenchmarkState<T: Bufferable> {
    sender: BufferSender<T>,
    receiver: BufferReceiver<T>,
    messages: Vec<T>,
    received: Vec<T>,
}

impl<T: Bufferable> BenchmarkState<T> {
    async fn write_then_read(mut self) -> Self {
        let message_count = self.messages.len();
        for message in self.messages.drain(..) {
            self.sender.send(message, None).await.unwrap();
        }
        self.sender
            .flush()
            .await
            .expect("buffer flush must succeed");

        for _ in 0..message_count {
            self.received
                .push(self.receiver.next().await.expect("record must exist"));
        }

        self
    }

    async fn write_and_read(mut self) -> Self {
        for message in self.messages.drain(..) {
            self.sender.send(message, None).await.unwrap();
            self.sender
                .flush()
                .await
                .expect("buffer flush must succeed");
            self.received.push(self.receiver.next().await.unwrap());
        }

        self
    }

    pub async fn setup_with<F>(
        variant: BufferType,
        total_records: usize,
        data_dir: Option<PathBuf>,
        id: String,
        mut make_record: F,
    ) -> Self
    where
        T: Clone,
        F: FnMut(usize) -> T,
    {
        let messages = (0..total_records).map(&mut make_record).collect();
        let received = Vec::with_capacity(total_records);

        let mut builder = TopologyBuilder::default();
        variant
            .add_to_builder(&mut builder, data_dir, id)
            .expect("should not fail to add variant to builder");
        let (sender, receiver) = builder
            .build(String::from("benches"), Span::none())
            .await
            .expect("should not fail to build topology");

        Self {
            sender,
            receiver,
            messages,
            received,
        }
    }
}

impl<const N: usize> BenchmarkState<Message<N>> {
    pub async fn setup(
        variant: BufferType,
        total_events: usize,
        data_dir: Option<PathBuf>,
        id: String,
    ) -> Self {
        Self::setup_with(variant, total_events, data_dir, id, |id| {
            Message::new(id as u64)
        })
        .await
    }
}

pub fn init_instrumentation() {
    let subscriber = tracing_subscriber::Registry::default().with(MetricsLayer::new());
    if tracing::subscriber::set_global_default(subscriber).is_ok() {
        let recorder = TracingContextLayer::all().layer(DebuggingRecorder::new());
        metrics::set_global_recorder(recorder).unwrap();
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

#[derive(Clone, Copy)]
pub enum Operation {
    WriteThenRead,
    WriteAndRead,
}

impl Operation {
    pub const fn name(self) -> &'static str {
        match self {
            Self::WriteThenRead => "write-then-read",
            Self::WriteAndRead => "write-and-read",
        }
    }

    pub async fn measure<T: Bufferable>(self, state: BenchmarkState<T>) -> BenchmarkState<T> {
        match self {
            Self::WriteThenRead => state.write_then_read().await,
            Self::WriteAndRead => state.write_and_read().await,
        }
    }
}
