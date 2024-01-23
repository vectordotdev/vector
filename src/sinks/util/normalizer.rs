use std::{
    pin::Pin,
    task::{ready, Context, Poll},
};

use futures_util::{stream::Fuse, Stream, StreamExt};
use pin_project::pin_project;
use vector_lib::event::Metric;

use super::buffer::metrics::{MetricNormalize, MetricNormalizer};

#[pin_project]
pub struct Normalizer<St, N>
where
    St: Stream,
{
    #[pin]
    stream: Fuse<St>,
    normalizer: MetricNormalizer<N>,
}

impl<St, N> Normalizer<St, N>
where
    St: Stream,
{
    pub fn new(stream: St, normalizer: N) -> Self {
        Self {
            stream: stream.fuse(),
            normalizer: MetricNormalizer::from(normalizer),
        }
    }
}

impl<St, N> Stream for Normalizer<St, N>
where
    St: Stream<Item = Metric>,
    N: MetricNormalize,
{
    type Item = Metric;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        loop {
            match ready!(this.stream.as_mut().poll_next(cx)) {
                Some(metric) => {
                    if let Some(normalized) = this.normalizer.normalize(metric) {
                        return Poll::Ready(Some(normalized));
                    }
                }
                None => return Poll::Ready(None),
            }
        }
    }
}
