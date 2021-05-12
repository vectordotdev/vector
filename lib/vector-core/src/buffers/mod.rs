mod acker;
#[cfg(feature = "disk-buffer")]
pub mod disk;

use crate::event::Event;
pub use acker::Acker;
use futures::{channel::mpsc, Sink, SinkExt, Stream};
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
#[cfg(feature = "disk-buffer")]
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

// NOTE unfortunately because we can't edit out a lifetime based on a feature
// flag we need two copies of `Variant` else the liftime being unused when
// 'disk-buffers' is flagged off will ding the build.

#[derive(Debug, Clone, Copy)]
#[cfg(not(feature = "disk-buffer"))]
pub enum Variant {
    Memory {
        max_events: usize,
        when_full: WhenFull,
    },
}

#[derive(Debug, Clone, Copy)]
#[cfg(feature = "disk-buffer")]
pub enum Variant<'a> {
    Memory {
        max_events: usize,
        when_full: WhenFull,
    },
    Disk {
        max_size: usize,
        when_full: WhenFull,
        data_dir: &'a Path,
        name: &'a str,
    },
}

/// Build a new buffer based on the passed `Variant`
///
/// # Errors
///
/// This function will fail only when creating a new disk buffer. Because of
/// legacy reasons the error is not a type but a `String`.
pub fn build(
    variant: Variant,
) -> Result<
    (
        BufferInputCloner,
        Box<dyn Stream<Item = Event> + Send>,
        Acker,
    ),
    String,
> {
    match variant {
        #[cfg(feature = "disk-buffer")]
        Variant::Disk {
            max_size,
            when_full,
            data_dir,
            name,
        } => {
            let buffer_dir = format!("{}_buffer", name);

            let (tx, rx, acker) =
                disk::open(data_dir, &buffer_dir, max_size).map_err(|error| error.to_string())?;

            let tx = BufferInputCloner::Disk(tx, when_full);
            Ok((tx, rx, acker))
        }
        Variant::Memory {
            max_events,
            when_full,
        } => {
            let (tx, rx) = mpsc::channel(max_events);
            let tx = BufferInputCloner::Memory(tx, when_full);
            let rx = Box::new(rx);
            Ok((tx, rx, Acker::Null))
        }
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum WhenFull {
    Block,
    DropNewest,
}

impl Default for WhenFull {
    fn default() -> Self {
        WhenFull::Block
    }
}

// Clippy warns that the `Disk` variant below is much larger than the
// `Memory` variant (currently 233 vs 25 bytes) and recommends boxing
// the large fields to reduce the total size.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum BufferInputCloner {
    Memory(mpsc::Sender<Event>, WhenFull),
    #[cfg(feature = "disk-buffer")]
    Disk(disk::Writer, WhenFull),
}

impl BufferInputCloner {
    pub fn get(&self) -> Box<dyn Sink<Event, Error = ()> + Send> {
        match self {
            BufferInputCloner::Memory(tx, when_full) => {
                let inner = tx
                    .clone()
                    .sink_map_err(|error| error!(message = "Sender error.", %error));
                if when_full == &WhenFull::DropNewest {
                    Box::new(DropWhenFull::new(inner))
                } else {
                    Box::new(inner)
                }
            }

            #[cfg(feature = "disk-buffer")]
            BufferInputCloner::Disk(writer, when_full) => {
                let inner = writer.clone();
                if when_full == &WhenFull::DropNewest {
                    Box::new(DropWhenFull::new(inner))
                } else {
                    Box::new(inner)
                }
            }
        }
    }
}

#[pin_project]
pub struct DropWhenFull<S> {
    #[pin]
    inner: S,
    drop: bool,
}

impl<S> DropWhenFull<S> {
    pub fn new(inner: S) -> Self {
        Self { inner, drop: false }
    }
}

impl<T, S: Sink<T> + Unpin> Sink<T> for DropWhenFull<S> {
    type Error = S::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        match this.inner.poll_ready(cx) {
            Poll::Ready(Ok(())) => {
                *this.drop = false;
                Poll::Ready(Ok(()))
            }
            Poll::Pending => {
                *this.drop = true;
                Poll::Ready(Ok(()))
            }
            error => error,
        }
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        if self.drop {
            debug!(
                message = "Shedding load; dropping event.",
                internal_log_rate_secs = 10
            );
            Ok(())
        } else {
            self.project().inner.start_send(item)
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

#[cfg(test)]
mod test {
    use super::{Acker, DropWhenFull};
    use futures::{channel::mpsc, future, task::AtomicWaker, Sink, Stream};
    use std::{
        sync::{atomic::AtomicUsize, Arc},
        task::Poll,
    };
    use tokio_test::task::spawn;

    #[tokio::test]
    async fn drop_when_full() {
        future::lazy(|cx| {
            let (tx, rx) = mpsc::channel(2);

            let mut tx = Box::pin(DropWhenFull::new(tx));

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
}
