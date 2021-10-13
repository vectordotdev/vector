use std::{pin::Pin, task::{Context, Poll, ready}};

use futures_util::Stream;
use vector_core::event::Metric;

use super::buffer::metrics::{MetricNormalize, MetricNormalizer};

pub struct Normalizer<St, N>
where
	N: MetricNormalize,
{
	stream: St,
	normalizer: MetricNormalizer<N>,
}

impl<St, N> Normalizer<St, N>
where
	N: MetricNormalize,
{
	pub fn new(stream: St) -> Normalizer<St, N>
	where
		N: MetricNormalize,
	{
		Self {
			stream,
			normalizer: MetricNormalizer::default(),
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
    }
}