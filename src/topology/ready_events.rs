use std::pin::Pin;

use futures::task::{Context, Poll};
use futures::{Stream, StreamExt};

use crate::event::{EventArray, EventContainer};

/// An adaptor for pulling an appropriately-sized `EventArray` from the stream.
///
/// This combinator will attempt to pull ready `Event`s from the stream
/// and buffer them into a local array. At most `capacity` `Event`s will
/// get buffered before an `EventArray` is yielded from the returned
/// stream. If underlying stream returns Poll::Pending, and collected
/// chunk is not empty, it will be immediately returned.
pub struct ReadyEvents<S> {
    inner: S,
    buffer: Option<EventArray>,
    capacity: usize,
}

impl<S> ReadyEvents<S> {
    fn push(&mut self, incoming: EventArray) -> Option<EventArray> {
        match self.buffer.take() {
            None => {
                let (result, buffer) = split_array(incoming, self.capacity);
                self.buffer = buffer;
                result
            }
            Some(buffered) => match (buffered, incoming) {
                (EventArray::Logs(mut buffered), EventArray::Logs(incoming)) => {
                    buffered.extend(incoming);
                    let (result, buffer) = split_buffer(buffered, self.capacity, EventArray::Logs);
                    self.buffer = buffer;
                    result
                }
                (EventArray::Metrics(mut buffered), EventArray::Metrics(incoming)) => {
                    buffered.extend(incoming);
                    let (result, buffer) =
                        split_buffer(buffered, self.capacity, EventArray::Metrics);
                    self.buffer = buffer;
                    result
                }
                (buffered, incoming) => {
                    self.buffer = Some(incoming);
                    Some(buffered)
                }
            },
        }
    }
}

/// Wrapper for `split_buffer` that handles the `EventArray` variant matching.
fn split_array(events: EventArray, capacity: usize) -> (Option<EventArray>, Option<EventArray>) {
    match events {
        EventArray::Logs(buffer) => split_buffer(buffer, capacity, EventArray::Logs),
        EventArray::Metrics(buffer) => split_buffer(buffer, capacity, EventArray::Metrics),
        EventArray::Traces(buffer) => split_buffer(buffer, capacity, EventArray::Traces),
    }
}

/// Return an optional result array if the `buffer` is at least
/// `capacity` elements long and an optional (new) array if there were
/// any elements remaining after splitting off that result. The `array`
/// parameter is used to map the result buffer into an `EventArray` for
/// convenience.
fn split_buffer<T>(
    mut buffer: Vec<T>,
    capacity: usize,
    array: impl Fn(Vec<T>) -> EventArray,
) -> (Option<EventArray>, Option<EventArray>) {
    if buffer.is_empty() {
        // This shouldn't happen, but be defensive anyways.
        (None, None)
    } else if buffer.len() < capacity {
        // The buffer is too small to split anything off.
        (None, Some(array(buffer)))
    } else if buffer.len() == capacity {
        // The buffer is exactly the size of the capacity.
        (Some(array(buffer)), None)
    } else {
        // The buffer is larger than the capacity, split it up.
        let other = buffer.split_off(capacity);
        (Some(array(buffer)), Some(array(other)))
    }
}

impl<S: Stream<Item = EventArray> + Unpin> Stream for ReadyEvents<S> {
    type Item = EventArray;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // If there's a stored buffer, see if there is enough already
        // buffered from a previous poll to return immediately.
        if let Some(buffer) = self.buffer.take() {
            if buffer.len() >= self.capacity {
                let (result, buffer) = split_array(buffer, self.capacity);
                self.buffer = buffer;
                return Poll::Ready(result); // Will always be `Some`
            }
            self.buffer = Some(buffer);
        }

        loop {
            match self.inner.poll_next_unpin(cx) {
                Poll::Ready(Some(events)) => {
                    if let Some(result) = self.push(events) {
                        return Poll::Ready(Some(result));
                    }
                }
                empty @ (Poll::Ready(None) | Poll::Pending) => {
                    return match self.buffer.take() {
                        Some(events) => Poll::Ready(Some(events)),
                        None => empty,
                    }
                }
            }
        }
    }
}

pub trait ReadyEventsExt<S> {
    fn ready_events(self, capacity: usize) -> ReadyEvents<S>;
}

impl<S: Stream<Item = EventArray> + Unpin> ReadyEventsExt<S> for S {
    fn ready_events(self, capacity: usize) -> ReadyEvents<S> {
        ReadyEvents {
            inner: self,
            buffer: None,
            capacity,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use futures::{channel::mpsc, poll, task::Poll, SinkExt, StreamExt};

    use super::*;
    use crate::event::{LogEvent, Metric, MetricKind, MetricValue};

    #[tokio::test]
    async fn ready_single() {
        let (mut tx, mut rx) = setup().await;
        tx.send(make_logs(0..1)).await.unwrap();

        receive_logs(&mut rx, 0..1).await;
        assert_eq!(poll!(rx.next()), Poll::Pending);
    }

    #[tokio::test]
    async fn ready_full() {
        let (mut tx, mut rx) = setup().await;
        tx.send(make_logs(0..3)).await.unwrap();

        receive_logs(&mut rx, 0..3).await;
        assert_eq!(poll!(rx.next()), Poll::Pending);
    }

    #[tokio::test]
    async fn ready_overfull() {
        let (mut tx, mut rx) = setup().await;
        tx.send(make_logs(0..5)).await.unwrap();

        receive_logs(&mut rx, 0..3).await;
        receive_logs(&mut rx, 3..5).await;
        assert_eq!(poll!(rx.next()), Poll::Pending);
    }

    #[tokio::test]
    async fn combines_events_partial() {
        let (mut tx, mut rx) = setup().await;
        tx.send(make_logs(0..1)).await.unwrap();
        tx.send(make_logs(1..2)).await.unwrap();

        receive_logs(&mut rx, 0..2).await;
        assert_eq!(poll!(rx.next()), Poll::Pending);
    }

    #[tokio::test]
    async fn combines_events_full() {
        let (mut tx, mut rx) = setup().await;
        tx.send(make_logs(0..1)).await.unwrap();
        tx.send(make_logs(1..2)).await.unwrap();
        tx.send(make_logs(2..3)).await.unwrap();

        receive_logs(&mut rx, 0..3).await;
        assert_eq!(poll!(rx.next()), Poll::Pending);
    }

    #[tokio::test]
    async fn combines_events_overfull() {
        let (mut tx, mut rx) = setup().await;
        tx.send(make_logs(0..1)).await.unwrap();
        tx.send(make_logs(1..2)).await.unwrap();
        tx.send(make_logs(2..3)).await.unwrap();
        tx.send(make_logs(3..4)).await.unwrap();

        receive_logs(&mut rx, 0..3).await;
        receive_logs(&mut rx, 3..4).await;
        assert_eq!(poll!(rx.next()), Poll::Pending);
    }

    #[tokio::test]
    async fn combines_events_overfull_2() {
        let (mut tx, mut rx) = setup().await;
        tx.send(make_logs(0..2)).await.unwrap();
        tx.send(make_logs(2..4)).await.unwrap();

        receive_logs(&mut rx, 0..3).await;
        receive_logs(&mut rx, 3..4).await;
        assert_eq!(poll!(rx.next()), Poll::Pending);
    }

    #[tokio::test]
    async fn closes_after_single() {
        let (mut tx, mut rx) = setup().await;
        tx.send(make_logs(0..1)).await.unwrap();
        drop(tx);

        receive_logs(&mut rx, 0..1).await;
        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }

    #[tokio::test]
    async fn closes_after_full() {
        let (mut tx, mut rx) = setup().await;
        tx.send(make_logs(0..3)).await.unwrap();
        drop(tx);

        receive_logs(&mut rx, 0..3).await;
        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }

    #[tokio::test]
    async fn closes_after_overfull() {
        let (mut tx, mut rx) = setup().await;
        tx.send(make_logs(0..5)).await.unwrap();
        drop(tx);

        receive_logs(&mut rx, 0..3).await;
        receive_logs(&mut rx, 3..5).await;
        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }

    #[tokio::test]
    async fn switches_types() {
        let (mut tx, mut rx) = setup().await;
        tx.send(make_logs(0..5)).await.unwrap();
        tx.send(make_metrics(5..10)).await.unwrap();

        receive_logs(&mut rx, 0..3).await;
        receive_logs(&mut rx, 3..5).await;
        receive_metrics(&mut rx, 5..8).await;
        receive_metrics(&mut rx, 8..10).await;
        assert_eq!(poll!(rx.next()), Poll::Pending);
    }

    async fn setup() -> (mpsc::Sender<EventArray>, impl Stream<Item = EventArray>) {
        let (tx, rx) = mpsc::channel(6);
        let mut rx = rx.ready_events(3);
        assert_eq!(Poll::Pending, poll!(rx.next()));
        (tx, rx)
    }

    async fn receive_logs(rx: &mut (impl Stream<Item = EventArray> + Unpin), range: Range<usize>) {
        let received = match poll!(rx.next()) {
            Poll::Ready(Some(t)) => t,
            result => panic!("Not ready: {:?}", result),
        };

        assert_eq!(received.len(), range.len());

        let mut received = received.into_events();
        range.for_each(|line| {
            let log = received.next().unwrap().into_log();
            assert_eq!(log["message"], format!("log #{}", line).into());
        });
    }

    async fn receive_metrics(
        rx: &mut (impl Stream<Item = EventArray> + Unpin),
        range: Range<usize>,
    ) {
        let received = match poll!(rx.next()) {
            Poll::Ready(Some(t)) => t,
            result => panic!("Not ready: {:?}", result),
        };

        assert_eq!(received.len(), range.len());

        let mut received = received.into_events();
        range.for_each(|line| {
            let metric = received.next().unwrap().into_metric();
            assert_eq!(metric.name(), format!("metric #{}", line));
        });
    }

    fn make_logs(range: Range<usize>) -> EventArray {
        range
            .map(|line| LogEvent::from(format!("log #{}", line)))
            .collect::<Vec<_>>()
            .into()
    }

    fn make_metrics(range: Range<usize>) -> EventArray {
        range
            .map(|value| {
                Metric::new(
                    format!("metric #{}", value),
                    MetricKind::Absolute,
                    MetricValue::Gauge {
                        value: value as f64,
                    },
                )
            })
            .collect::<Vec<_>>()
            .into()
    }
}
