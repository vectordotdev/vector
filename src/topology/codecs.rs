use futures::{stream::BoxStream, Stream, StreamExt};
use vector_core::event::Event;

pub struct Codecs {
    stream: BoxStream<'static, Event>,
}

impl Codecs {
    pub fn new(
        codecs: &crate::config::codec::CodecsConfig,
        stream: impl Stream<Item = Event> + Send + 'static,
    ) -> Self {
        let mut stream: BoxStream<Event> = stream.boxed();

        for codec in &codecs.codecs {
            stream = codec.codec(stream).boxed();
        }

        Self { stream }
    }
}

impl Stream for Codecs {
    type Item = Event;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }
}
