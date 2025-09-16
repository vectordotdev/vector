use std::{
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
};

use futures_util::{Stream, StreamExt, stream::Fuse};
use pin_project::pin_project;
use vector_lib::event::Metric;

use crate::sinks::util::buffer::metrics::{
    DefaultNormalizerSettings, MetricNormalize, MetricNormalizer, NormalizerConfig,
};

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

    pub fn new_with_ttl(stream: St, normalizer: N, ttl: Duration) -> Self {
        // Create a new MetricSetConfig with the proper settings type
        let config = NormalizerConfig::<DefaultNormalizerSettings> {
            time_to_live: Some(ttl.as_secs()),
            max_bytes: None,
            max_events: None,
            _d: std::marker::PhantomData,
        };
        Self {
            stream: stream.fuse(),
            normalizer: MetricNormalizer::with_config(normalizer, config),
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
