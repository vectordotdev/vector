mod common;
mod model;

use crate::{Acker, DropWhenFull};
use futures::task::Poll;
use futures::{channel::mpsc, future, task::AtomicWaker};
use futures::{Sink, Stream};
#[cfg(loom)]
use loom::sync::{atomic::AtomicUsize, Arc};
#[cfg(not(loom))]
use std::sync::{atomic::AtomicUsize, Arc};
use tokio_test::task::spawn;

// #[tokio::test]
// #[allow(clippy::semicolon_if_nothing_returned)] // appears to be a false positive as there is a ;
// async fn drop_when_full() {
//     future::lazy(|cx| {
//         let (tx, rx) = mpsc::channel(2);

//         let mut tx = Box::pin(DropWhenFull::new(tx));

//         assert_eq!(tx.as_mut().poll_ready(cx), Poll::Ready(Ok(())));
//         assert_eq!(tx.as_mut().start_send(1), Ok(()));
//         assert_eq!(tx.as_mut().poll_ready(cx), Poll::Ready(Ok(())));
//         assert_eq!(tx.as_mut().start_send(2), Ok(()));
//         assert_eq!(tx.as_mut().poll_ready(cx), Poll::Ready(Ok(())));
//         assert_eq!(tx.as_mut().start_send(3), Ok(()));
//         assert_eq!(tx.as_mut().poll_ready(cx), Poll::Ready(Ok(())));
//         assert_eq!(tx.as_mut().start_send(4), Ok(()));

//         let mut rx = Box::pin(rx);

//         assert_eq!(rx.as_mut().poll_next(cx), Poll::Ready(Some(1)));
//         assert_eq!(rx.as_mut().poll_next(cx), Poll::Ready(Some(2)));
//         assert_eq!(rx.as_mut().poll_next(cx), Poll::Ready(Some(3)));
//         assert_eq!(rx.as_mut().poll_next(cx), Poll::Pending);
//     })
//     .await;
// }
