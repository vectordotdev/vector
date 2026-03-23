use std::{future::Future, pin::Pin};

use futures::{
    Stream,
    stream::{FuturesOrdered, FuturesUnordered},
    task::{Context, Poll},
};

/// Wraps either a [`FuturesOrdered`] or [`FuturesUnordered`] with a unified
/// interface, allowing the caller to choose between ordering and throughput
/// at runtime without duplicating the polling loop.
pub enum InFlightQueue<Fut: Future> {
    Ordered(FuturesOrdered<Fut>),
    Unordered(FuturesUnordered<Fut>),
}

impl<Fut: Future> InFlightQueue<Fut> {
    pub fn new(preserve_ordering: bool) -> Self {
        if preserve_ordering {
            Self::Ordered(FuturesOrdered::new())
        } else {
            Self::Unordered(FuturesUnordered::new())
        }
    }

    pub fn push(&mut self, fut: Fut) {
        match self {
            Self::Ordered(q) => q.push_back(fut),
            Self::Unordered(q) => q.push(fut),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Ordered(q) => q.len(),
            Self::Unordered(q) => q.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Ordered(q) => q.is_empty(),
            Self::Unordered(q) => q.is_empty(),
        }
    }
}

impl<Fut: Future> Stream for InFlightQueue<Fut> {
    type Item = Fut::Output;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            Self::Ordered(q) => Pin::new(q).poll_next(cx),
            Self::Unordered(q) => Pin::new(q).poll_next(cx),
        }
    }
}
