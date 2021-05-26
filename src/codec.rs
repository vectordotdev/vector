use futures::{stream::BoxStream, Stream, StreamExt};
use vector_core::event::Event;

pub trait Codec: Stream<Item = Event> {
    fn new(stream: impl Stream<Item = Event> + Send + 'static) -> Self;
}

pub struct NoopCodec {
    stream: BoxStream<'static, Event>,
}

impl Stream for NoopCodec {
    type Item = Event;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }
}

impl Codec for NoopCodec {
    fn new(stream: impl Stream<Item = Event> + Send + 'static) -> Self {
        Self {
            stream: stream.boxed(),
        }
    }
}
