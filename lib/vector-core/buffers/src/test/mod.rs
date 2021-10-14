mod common;
mod model;

use crate::buffer_usage_data::BufferUsageData;
use crate::{Acker, DropWhenFull, WhenFull};
use futures::task::Poll;
use futures::{channel::mpsc, future, task::AtomicWaker};
use futures::{Sink, Stream};
use std::sync::{atomic::AtomicUsize, Arc};
use tokio_test::task::spawn;
use tracing::Span;

#[tokio::test]
#[allow(clippy::semicolon_if_nothing_returned)] // appears to be a false positive as there is a ;
async fn drop_when_full() {
    future::lazy(|cx| {
        let (tx, rx) = mpsc::channel(2);

        let mut tx = Box::pin(DropWhenFull::new(
            tx,
            BufferUsageData::new(WhenFull::DropNewest, Span::none(), None, Some(2)),
        ));

        assert_eq!(tx.as_mut().poll_ready(cx), Poll::Ready(Ok(())));
        assert_eq!(tx.as_mut().start_send(1), Ok(()));
        assert_eq!(tx.as_mut().poll_ready(cx), Poll::Ready(Ok(())));
        assert_eq!(tx.as_mut().start_send(2), Ok(()));
        assert_eq!(tx.as_mut().poll_ready(cx), Poll::Ready(Ok(())));
        assert_eq!(tx.as_mut().start_send(3), Ok(()));
        assert_eq!(tx.as_mut().poll_ready(cx), Poll::Ready(Ok(())));
        assert_eq!(tx.as_mut().start_send(4), Ok(()));

        let mut rx = Box::pin(rx);

        assert_eq!(rx.as_mut().poll_next(cx), Poll::Ready(Some(1)));
        assert_eq!(rx.as_mut().poll_next(cx), Poll::Ready(Some(2)));
        assert_eq!(rx.as_mut().poll_next(cx), Poll::Ready(Some(3)));
        assert_eq!(rx.as_mut().poll_next(cx), Poll::Pending);
    })
    .await;
}

#[test]
fn ack_with_none() {
    let counter = Arc::new(AtomicUsize::new(0));
    let task = Arc::new(AtomicWaker::new());
    let acker = Acker::Disk(counter, Arc::clone(&task));

    let mut mock = spawn(future::poll_fn::<(), _>(|cx| {
        task.register(cx.waker());
        Poll::Pending
    }));
    let _ = mock.poll();

    assert!(!mock.is_woken());
    acker.ack(0);
    assert!(!mock.is_woken());
    acker.ack(1);
    assert!(mock.is_woken());
}
