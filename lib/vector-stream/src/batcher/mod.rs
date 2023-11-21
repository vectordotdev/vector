pub mod config;
pub mod data;
pub mod limiter;

use std::{
    pin::Pin,
    task::{ready, Context, Poll},
};

pub use config::BatchConfig;
use futures::{
    stream::{Fuse, Stream},
    Future, StreamExt,
};
use pin_project::pin_project;
use tokio::time::Sleep;

#[pin_project]
pub struct Batcher<S, C> {
    state: C,

    #[pin]
    /// The stream this `Batcher` wraps
    stream: Fuse<S>,

    #[pin]
    timer: Maybe<Sleep>,
}

/// An `Option`, but with pin projection
#[pin_project(project = MaybeProj)]
pub enum Maybe<T> {
    Some(#[pin] T),
    None,
}

impl<S, C> Batcher<S, C>
where
    S: Stream,
    C: BatchConfig<S::Item>,
{
    pub fn new(stream: S, config: C) -> Self {
        Self {
            state: config,
            stream: stream.fuse(),
            timer: Maybe::None,
        }
    }
}

impl<S, C> Stream for Batcher<S, C>
where
    S: Stream,
    C: BatchConfig<S::Item>,
{
    type Item = C::Batch;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let mut this = self.as_mut().project();
            match this.stream.poll_next(cx) {
                Poll::Ready(None) => {
                    return {
                        if this.state.len() == 0 {
                            Poll::Ready(None)
                        } else {
                            Poll::Ready(Some(this.state.take_batch()))
                        }
                    }
                }
                Poll::Ready(Some(item)) => {
                    let (item_fits, item_metadata) = this.state.item_fits_in_batch(&item);
                    if item_fits {
                        this.state.push(item, item_metadata);
                        if this.state.is_batch_full() {
                            this.timer.set(Maybe::None);
                            return Poll::Ready(Some(this.state.take_batch()));
                        } else if this.state.len() == 1 {
                            this.timer
                                .set(Maybe::Some(tokio::time::sleep(this.state.timeout())));
                        }
                    } else {
                        let output = Poll::Ready(Some(this.state.take_batch()));
                        this.state.push(item, item_metadata);
                        this.timer
                            .set(Maybe::Some(tokio::time::sleep(this.state.timeout())));
                        return output;
                    }
                }
                Poll::Pending => {
                    return {
                        if let MaybeProj::Some(timer) = this.timer.as_mut().project() {
                            ready!(timer.poll(cx));
                            this.timer.set(Maybe::None);
                            debug_assert!(
                                this.state.len() != 0,
                                "timer should have been cancelled"
                            );
                            Poll::Ready(Some(this.state.take_batch()))
                        } else {
                            Poll::Pending
                        }
                    }
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

#[cfg(test)]
#[allow(clippy::similar_names)]
mod test {
    use std::{num::NonZeroUsize, time::Duration};

    use futures::stream;

    use super::*;
    use crate::BatcherSettings;

    #[tokio::test]
    async fn item_limit() {
        let stream = stream::iter([1, 2, 3]);
        let settings = BatcherSettings::new(
            Duration::from_millis(100),
            NonZeroUsize::new(10000).unwrap(),
            NonZeroUsize::new(2).unwrap(),
        );
        let batcher = Batcher::new(stream, settings.as_item_size_config(|x: &u32| *x as usize));
        let batches: Vec<_> = batcher.collect().await;
        assert_eq!(batches, vec![vec![1, 2], vec![3],]);
    }

    #[tokio::test]
    async fn size_limit() {
        let batcher = Batcher::new(
            stream::iter([1, 2, 3, 4, 5, 6, 2, 3, 1]),
            BatcherSettings::new(
                Duration::from_millis(100),
                NonZeroUsize::new(5).unwrap(),
                NonZeroUsize::new(100).unwrap(),
            )
            .as_item_size_config(|x: &u32| *x as usize),
        );
        let batches: Vec<_> = batcher.collect().await;
        assert_eq!(
            batches,
            vec![
                vec![1, 2],
                vec![3],
                vec![4],
                vec![5],
                vec![6],
                vec![2, 3],
                vec![1],
            ]
        );
    }

    #[tokio::test]
    async fn timeout_limit() {
        tokio::time::pause();

        let timeout = Duration::from_millis(100);
        let stream = stream::iter([1, 2]).chain(stream::pending());
        let batcher = Batcher::new(
            stream,
            BatcherSettings::new(
                timeout,
                NonZeroUsize::new(5).unwrap(),
                NonZeroUsize::new(100).unwrap(),
            )
            .as_item_size_config(|x: &u32| *x as usize),
        );

        tokio::pin!(batcher);
        let mut next = batcher.next();
        assert_eq!(futures::poll!(&mut next), Poll::Pending);
        tokio::time::advance(timeout).await;
        let batch = next.await;
        assert_eq!(batch, Some(vec![1, 2]));
    }
}
