use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::{ready, stream::Fuse, Stream, StreamExt};
use pin_project::pin_project;
use vector_core::event::Metric;

use super::buffer::metrics::{MetricNormalize, MetricSet};

#[pin_project]
pub struct Normalizer<St, N>
where
    St: Stream,
{
    #[pin]
    stream: Fuse<St>,
    metric_set: MetricSet,
    _normalizer: PhantomData<N>,
}

impl<St, N> Normalizer<St, N>
where
    St: Stream,
{
    pub fn new(stream: St) -> Self {
        Self {
            stream: stream.fuse(),
            metric_set: MetricSet::default(),
            _normalizer: PhantomData,
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
                    if let Some(normalized) = N::apply_state(this.metric_set, metric) {
                        return Poll::Ready(Some(normalized));
                    }
                }
                None => return Poll::Ready(None),
            }
        }
    }
}
