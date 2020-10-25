use crate::{buffers::Acker, Event};
use bytes::Bytes;
use futures::{ready, Sink, Stream};
use pin_project::pin_project;
use std::{
    marker::Unpin,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};
use tokio::io::AsyncWrite;
use tokio_util::codec::{BytesCodec, FramedWrite};

const MAX_PENDING_ITEMS: usize = 10_000;

pub struct EventsCounter {
    sizes: Mutex<Vec<usize>>,
    acker: Acker,
    encode_event_inner: Box<dyn Fn(Event) -> Option<Bytes> + Send + Sync>,
    on_success: Box<dyn Fn(usize) + Send + Sync>,
}

impl EventsCounter {
    pub fn new(
        acker: Acker,
        encode_event: impl Fn(Event) -> Option<Bytes> + Send + Sync + 'static,
        on_success: impl Fn(usize) + Send + Sync + 'static,
    ) -> Self {
        Self {
            sizes: Mutex::new(Vec::with_capacity(MAX_PENDING_ITEMS)),
            acker,
            encode_event_inner: Box::new(encode_event),
            on_success: Box::new(on_success),
        }
    }

    pub fn encode_event(&self, event: Event) -> Option<Bytes> {
        let mut sizes = self.sizes.lock().unwrap();
        let bytes = (self.encode_event_inner)(event);
        sizes.push(bytes.as_ref().map_or(0, |bytes| bytes.len()));
        bytes
    }

    fn is_full(&self) -> bool {
        let sizes = self.sizes.lock().unwrap();
        sizes.len() >= MAX_PENDING_ITEMS
    }

    fn ack(&self, count: usize) {
        let mut sizes = self.sizes.lock().unwrap();
        assert!(
            count <= sizes.len(),
            "can not ack more then consumed events"
        );
        self.acker.ack(count);
        for size in sizes.drain(..count) {
            (self.on_success)(size);
        }
    }

    pub fn ack_rest(&self) {
        let count = self.sizes.lock().unwrap().len();
        self.ack(count);
    }
}

#[pin_project]
pub struct EncodeEventStream<'a, S> {
    #[pin]
    inner: &'a mut S,
    encode_event: Box<dyn Fn(Event) -> Option<Bytes> + Send>,
}

impl<'a, S> EncodeEventStream<'a, S>
where
    S: Stream<Item = Event>,
{
    pub fn new(
        inner: &'a mut S,
        encode_event: impl Fn(Event) -> Option<Bytes> + Send + 'static,
    ) -> Self {
        Self {
            inner,
            encode_event: Box::new(encode_event),
        }
    }
}

impl<S> Stream for EncodeEventStream<'_, S>
where
    S: Stream<Item = Event> + Unpin,
{
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match ready!(self.as_mut().project().inner.poll_next(cx)) {
                Some(event) => {
                    if let Some(bytes) = (self.encode_event)(event) {
                        return Poll::Ready(Some(Ok(bytes)));
                    }
                }
                None => return Poll::Ready(None),
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

pub enum ShutdownCheck {
    Error(std::io::Error),
    Close,
    Alive,
}

#[pin_project]
pub struct BytesSink<T> {
    #[pin]
    inner: FramedWrite<T, BytesCodec>,
    shutdown_check: Box<dyn Fn(&mut T) -> ShutdownCheck + Send>,
    events_count: usize,
    events_counter: Arc<EventsCounter>,
}

impl<T: AsyncWrite> BytesSink<T> {
    pub fn new(
        inner: T,
        shutdown_check: Box<dyn Fn(&mut T) -> ShutdownCheck + Send>,
        events_counter: Arc<EventsCounter>,
    ) -> Self {
        Self {
            inner: FramedWrite::new(inner, BytesCodec::new()),
            shutdown_check,
            events_count: 0,
            events_counter,
        }
    }

    fn ack(&mut self) {
        self.events_counter.ack(self.events_count);
        self.events_count = 0;
    }
}

impl<T> Sink<Bytes> for BytesSink<T>
where
    T: AsyncWrite + Unpin,
{
    type Error = <FramedWrite<T, BytesCodec> as Sink<Bytes>>::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let pinned = self.as_mut().project();
        match (pinned.shutdown_check)(pinned.inner.get_mut().get_mut()) {
            ShutdownCheck::Error(error) => return Poll::Ready(Err(error)), // TODO: add custom?
            ShutdownCheck::Close => {
                if let Err(error) = ready!(self.as_mut().poll_flush(cx)) {
                    return Poll::Ready(Err(error));
                }

                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "custom close",
                )));
            }
            ShutdownCheck::Alive => {}
        }

        if self.as_mut().project().events_counter.is_full() {
            if let Err(error) = ready!(self.as_mut().poll_flush(cx)) {
                return Poll::Ready(Err(error));
            }
        }

        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        let pinned = self.project();
        *pinned.events_count += 1;
        pinned.inner.start_send(item)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let result = ready!(self.as_mut().project().inner.poll_flush(cx));
        self.as_mut().get_mut().ack();
        Poll::Ready(result)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let result = ready!(self.as_mut().project().inner.poll_close(cx));
        self.as_mut().get_mut().ack();
        Poll::Ready(result)
    }
}
