use buffers::encoding::{DecodeBytes, EncodeBytes};
use buffers::topology::builder::{IntoBuffer, TopologyBuilder};
use buffers::topology::channel::{BufferReceiver, BufferSender};
use buffers::BufferType;
use bytes::{Buf, BufMut};
use core_common::byte_size_of::ByteSizeOf;
use futures::task::{noop_waker, Context, Poll};
use futures::{Sink, SinkExt, Stream};
use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
use metrics_util::layers::Layer;
use metrics_util::DebuggingRecorder;
use std::path::PathBuf;
use std::pin::Pin;
use std::{error, fmt};
use tracing::Span;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;

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

impl<const N: usize> ByteSizeOf for Message<N> {
    fn allocated_bytes(&self) -> usize {
        0
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

impl<const N: usize> EncodeBytes<Message<N>> for Message<N> {
    type Error = EncodeError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::Error>
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
}

impl<const N: usize> DecodeBytes<Message<N>> for Message<N> {
    type Error = DecodeError;

    fn decode<B>(mut buffer: B) -> Result<Self, Self::Error>
    where
        B: Buf,
        Self: Sized,
    {
        let id = buffer.get_u64();
        for _ in 0..N {
            // this covers self._padding
            let _ = buffer.get_u64();
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
) -> (
    BufferSender<Message<N>>,
    BufferReceiver<Message<N>>,
    Vec<Message<N>>,
) {
    let mut messages: Vec<Message<N>> = Vec::with_capacity(total_events);
    for i in 0..total_events {
        messages.push(Message::new(i as u64));
    }

    let mut builder = TopologyBuilder::new();
    variant
        .add_to_builder(&mut builder, data_dir, String::from("benchies"))
        .expect("should not fail to add variant to builder");
    let (tx, rx, _acker) = builder
        .build(Span::none())
        .await
        .expect("should not fail to build topology");

    (tx, rx, messages)
}

fn send_msg<const N: usize>(
    msg: Message<N>,
    mut sink: Pin<&mut (dyn Sink<Message<N>, Error = ()> + Unpin + Send)>,
    context: &mut Context,
) {
    match sink.as_mut().poll_ready(context) {
        Poll::Ready(Ok(())) => match sink.as_mut().start_send(msg) {
            Ok(()) => match sink.as_mut().poll_flush(context) {
                Poll::Ready(Ok(())) => {}
                _ => unreachable!(),
            },
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

#[inline]
fn consume<T>(mut stream: Pin<&mut (dyn Stream<Item = T> + Unpin + Send)>, context: &mut Context) {
    while let Poll::Ready(Some(_)) = stream.as_mut().poll_next(context) {}
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

#[allow(clippy::type_complexity)]
pub fn wtr_measurement<const N: usize>(
    input: (
        Pin<Box<dyn Sink<Message<N>, Error = ()> + Unpin + Send>>,
        Pin<Box<dyn Stream<Item = Message<N>> + Unpin + Send>>,
        Vec<Message<N>>,
    ),
) {
    {
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);

        let mut sink = input.0;
        for msg in input.2.into_iter() {
            send_msg(msg, sink.as_mut(), &mut context)
        }
    }

    {
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);

        let mut stream = input.1;
        consume(stream.as_mut(), &mut context)
    }
}

#[allow(clippy::type_complexity)]
pub fn war_measurement<const N: usize>(
    input: (
        Pin<Box<dyn Sink<Message<N>, Error = ()> + Unpin + Send>>,
        Pin<Box<dyn Stream<Item = Message<N>> + Unpin + Send>>,
        Vec<Message<N>>,
    ),
) {
    let snd_waker = noop_waker();
    let mut snd_context = Context::from_waker(&snd_waker);

    let rcv_waker = noop_waker();
    let mut rcv_context = Context::from_waker(&rcv_waker);

    let mut stream = input.1;
    let mut sink = input.0;
    for msg in input.2.into_iter() {
        send_msg(msg, sink.as_mut(), &mut snd_context);
        consume(stream.as_mut(), &mut rcv_context)
    }
}
