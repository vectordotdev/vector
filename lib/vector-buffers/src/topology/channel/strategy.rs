use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::Stream;

pub enum StrategyResult<T> {
    Primary(T),
    Secondary(T),
    Neither,
}

impl<T> StrategyResult<T> {
    fn map(item: Option<T>, primary: bool) -> Self {
        match item {
            None => StrategyResult::Neither,
            Some(item) => {
                if primary {
                    StrategyResult::Primary(item)
                } else {
                    StrategyResult::Secondary(item)
                }
            }
        }
    }
}

/// Strategy for polling the two streams of a [`BufferReceiver`].
///
/// Currently defines a round-robin strategy that toggles between the two input streams.  The
/// streams must be provided same order each time to ensure the internal state is consistent with
/// the last time it was called.
#[derive(Debug, Default)]
pub struct PollStrategy {
    poll_primary_first: bool,
}

impl PollStrategy {
    pub fn poll_streams<St1, St2, I>(
        &mut self,
        primary: Pin<&mut St1>,
        secondary: Option<Pin<&mut St2>>,
        cx: &mut Context<'_>,
    ) -> Poll<StrategyResult<I>>
    where
        St1: Stream<Item = I>,
        St2: Stream<Item = I>,
    {
        let result = match secondary {
            // Secondary stream isn't present, so just poll the primary stream.
            None => primary.poll_next(cx).map(|i| StrategyResult::map(i, true)),
            Some(secondary) => {
                // Both streams are present, so we just round-robin the ordering.
                if self.poll_primary_first {
                    poll_streams_inner(primary, secondary, true, cx)
                } else {
                    poll_streams_inner(secondary, primary, false, cx)
                }
            }
        };

        // Toggle our poll order for next time.
        self.poll_primary_first = !self.poll_primary_first;

        result
    }
}

fn poll_streams_inner<St1, St2, I>(
    primary: Pin<&mut St1>,
    secondary: Pin<&mut St2>,
    primary_first: bool,
    cx: &mut Context<'_>,
) -> Poll<StrategyResult<I>>
where
    St1: Stream<Item = I>,
    St2: Stream<Item = I>,
{
    match primary.poll_next(cx) {
        // Primary stream had an item for us, so pass it back.
        Poll::Ready(Some(item)) => Poll::Ready(StrategyResult::map(Some(item), primary_first)),
        // Primary stream has either ended or has no item for us currently, so try the secondary
        // stream now.
        p @ (Poll::Ready(None) | Poll::Pending) => match secondary.poll_next(cx) {
            // Secondary stream had an item for us, so pass it back.
            Poll::Ready(Some(item)) => Poll::Ready(StrategyResult::map(Some(item), !primary_first)),
            // Secondary stream has ended, so if the primary stream has also ended, push that to
            // the caller, otherwise just let them know we're still pending.
            Poll::Ready(None) => {
                if p.is_pending() {
                    Poll::Pending
                } else {
                    Poll::Ready(StrategyResult::Neither)
                }
            }
            // Secondary stream has no item for us currently, so the caller can still poll.
            Poll::Pending => Poll::Pending,
        },
    }
}
