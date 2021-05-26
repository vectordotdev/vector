use buffers::bytes::{DecodeBytes, EncodeBytes};
use buffers::{self, Variant};
use bytes::{Buf, BufMut};
use futures::task::{noop_waker, Context, Poll};
use futures::{Sink, Stream};
use std::fmt;
use std::pin::Pin;

#[derive(Clone, Copy)]
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

#[derive(Debug)]
pub enum EncodeError {}

#[derive(Debug)]
pub enum DecodeError {}

impl fmt::Display for DecodeError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}

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

#[allow(clippy::type_complexity)]
pub fn setup<const N: usize>(
    max_events: usize,
    variant: Variant,
) -> (
    Box<dyn Sink<Message<N>, Error = ()> + Unpin + Send>,
    Box<dyn Stream<Item = Message<N>> + Unpin + Send>,
    Vec<Message<N>>,
) {
    let mut messages: Vec<Message<N>> = Vec::with_capacity(max_events);
    for i in 0..max_events {
        messages.push(Message::new(i as u64));
    }

    let (tx, rx, _) = buffers::build::<Message<N>>(variant).unwrap();
    (tx.get(), rx, messages)
}

fn send_msg<const N: usize>(
    msg: Message<N>,
    sink: &mut (dyn Sink<Message<N>, Error = ()> + Unpin + Send),
    context: &mut Context,
) {
    match Sink::poll_ready(Pin::new(sink), context) {
        Poll::Ready(Ok(())) => match Sink::start_send(Pin::new(sink), msg) {
            Ok(()) => match Sink::poll_flush(Pin::new(sink), context) {
                Poll::Ready(Ok(())) => {}
                _ => unreachable!(),
            },
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

fn read_all_msg<const N: usize>(
    stream: &mut (dyn Stream<Item = Message<N>> + Unpin + Send),
    context: &mut Context,
) {
    while let Poll::Ready(Some(_)) = Stream::poll_next(Pin::new(stream), context) {}
}

//
// Measurements
//
// The nature of our buffer is such that the underlying representation is hidden
// behind an abstract interface. As a happy consequence of this our benchmark
// measurements are common. "Write Then Read" writes all messages into the
// buffer and then reads them out. "Write And Read" writes a message and then
// reads it from the buffer. Measurement is done without a runtime avoiding
// conflating the overhead of the runtime with our buffer code. This,
// admittedly, is tough to read.
//

#[allow(clippy::type_complexity)]
pub fn wtr_measurement<const N: usize>(
    mut input: (
        Box<dyn Sink<Message<N>, Error = ()> + Unpin + Send>,
        Box<dyn Stream<Item = Message<N>> + Unpin + Send>,
        Vec<Message<N>>,
    ),
) {
    {
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);

        let sink = input.0.as_mut();
        for msg in input.2.into_iter() {
            send_msg(msg, sink, &mut context)
        }
    }

    {
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);

        let stream = input.1.as_mut();
        read_all_msg(stream, &mut context)
    }
}

#[allow(clippy::type_complexity)]
pub fn war_measurement<const N: usize>(
    mut input: (
        Box<dyn Sink<Message<N>, Error = ()> + Unpin + Send>,
        Box<dyn Stream<Item = Message<N>> + Unpin + Send>,
        Vec<Message<N>>,
    ),
) {
    let snd_waker = noop_waker();
    let mut snd_context = Context::from_waker(&snd_waker);

    let rcv_waker = noop_waker();
    let mut rcv_context = Context::from_waker(&rcv_waker);

    let stream = input.1.as_mut();
    let sink = input.0.as_mut();
    for msg in input.2.into_iter() {
        send_msg(msg, sink, &mut snd_context);
        read_all_msg(stream, &mut rcv_context)
    }
}
