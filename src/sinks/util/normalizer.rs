use std::{pin::Pin, task::{Context, Poll}};

use futures_util::{Stream, ready};
use vector_core::event::Metric;

use super::buffer::metrics::{MetricNormalize, MetricNormalizer};

pub struct Normalizer<St, N> {
	stream: St,
	normalizer: N,
	metric_set: MetricSet,
}

impl<St, N> Normalizer<St, N> {
	pub fn new(stream: St, normalizer: N) -> Self {
		Self {
			stream,
			normalizer,
			metric_set: MetricSet::default(),
		}
	}
}

impl<St, N> Stream for Normalizer<St, N>
where
	St: Stream<Item = Metric> + Unpin,
	N: MetricNormalize,
{
	type Item = Metric;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let metric = ready!(self.stream.poll_next(cx));
		let result = N::apply_state(&mut self.metric_set, metric);
    }
}