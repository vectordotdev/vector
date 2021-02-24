use crate::{config::Resource, internal_events::EventOut, Event};
#[cfg(feature = "leveldb")]
use futures::compat::{Sink01CompatExt, Stream01CompatExt};
use futures::{channel::mpsc, Sink, SinkExt, Stream};
use futures01::task::AtomicTask;
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
#[cfg(feature = "leveldb")]
use tokio::stream::StreamExt;

#[cfg(feature = "leveldb")]
pub mod disk;

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum BufferConfig {
    Memory {
        #[serde(default = "BufferConfig::memory_max_events")]
        max_events: usize,
        #[serde(default)]
        when_full: WhenFull,
    },
    #[cfg(feature = "leveldb")]
    Disk {
        max_size: usize,
        #[serde(default)]
        when_full: WhenFull,
    },
}

impl Default for BufferConfig {
    fn default() -> Self {
        BufferConfig::Memory {
            max_events: BufferConfig::memory_max_events(),
            when_full: Default::default(),
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

#[derive(Clone)]
pub enum BufferInputCloner {
    Memory(mpsc::Sender<Event>, WhenFull),
    #[cfg(feature = "leveldb")]
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

            #[cfg(feature = "leveldb")]
            BufferInputCloner::Disk(writer, when_full) => {
                let inner = writer.clone().sink_compat();
                if when_full == &WhenFull::DropNewest {
                    Box::new(DropWhenFull::new(inner))
                } else {
                    Box::new(inner)
                }
            }
        }
    }
}

impl BufferConfig {
    #[inline]
    const fn memory_max_events() -> usize {
        500
    }

    #[cfg_attr(not(feature = "leveldb"), allow(unused))]
    pub fn build(
        &self,
        data_dir: &Option<PathBuf>,
        sink_name: &str,
    ) -> Result<
        (
            BufferInputCloner,
            Box<dyn Stream<Item = Event> + Send>,
            Acker,
        ),
        String,
    > {
        match &self {
            BufferConfig::Memory {
                max_events,
                when_full,
            } => {
                let (tx, rx) = mpsc::channel(*max_events);
                let tx = BufferInputCloner::Memory(tx, *when_full);
                let rx = Box::new(rx);
                Ok((tx, rx, Acker::Null))
            }

            #[cfg(feature = "leveldb")]
            BufferConfig::Disk {
                max_size,
                when_full,
            } => {
                let data_dir = data_dir
                    .as_ref()
                    .ok_or_else(|| "Must set data_dir to use on-disk buffering.".to_string())?;
                let buffer_dir = format!("{}_buffer", sink_name);

                let (tx, rx, acker) = disk::open(&data_dir, buffer_dir.as_ref(), *max_size)
                    .map_err(|error| error.to_string())?;
                let tx = BufferInputCloner::Disk(tx, *when_full);
                let rx = Box::new(
                    rx.compat()
                        .take_while(|event| event.is_ok())
                        .map(|event| event.unwrap()),
                );
                Ok((tx, rx, acker))
            }
        }
    }

    /// Resources that the sink is using.
    #[cfg_attr(not(feature = "leveldb"), allow(unused))]
    pub fn resources(&self, sink_name: &str) -> Vec<Resource> {
        match self {
            BufferConfig::Memory { .. } => Vec::new(),
            #[cfg(feature = "leveldb")]
            BufferConfig::Disk { .. } => vec![Resource::DiskBuffer(sink_name.to_string())],
        }
    }
}

#[derive(Debug, Clone)]
pub enum Acker {
    Disk(Arc<AtomicUsize>, Arc<AtomicTask>),
    Null,
}

impl Acker {
    // This method should be called by a sink to indicate that it has successfully
    // flushed the next `num` events from its input stream. If there are events that
    // have flushed, but events that came before them in the stream have not been flushed,
    // the later events must _not_ be acked until all preceding elements are also acked.
    // This is primary used by the on-disk buffer to know which events are okay to
    // delete from disk.
    pub fn ack(&self, num: usize) {
        // Only ack items if the amount to ack is larger than zero.
        if num > 0 {
            match self {
                Acker::Null => {}
                Acker::Disk(counter, notifier) => {
                    counter.fetch_add(num, Ordering::Relaxed);
                    notifier.notify();
                }
            }
            emit!(EventOut { count: num });
        }
    }

    pub fn new_for_testing() -> (Self, Arc<AtomicUsize>) {
        let ack_counter = Arc::new(AtomicUsize::new(0));
        let notifier = Arc::new(AtomicTask::new());
        let acker = Acker::Disk(Arc::clone(&ack_counter), Arc::clone(&notifier));

        (acker, ack_counter)
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
    use super::{Acker, BufferConfig, DropWhenFull, WhenFull};
    use crate::sink::BoundedSink;
    use futures::{future, Sink, Stream};
    use futures01::task::AtomicTask;
    use std::{
        sync::{atomic::AtomicUsize, Arc},
        task::Poll,
    };
    use tokio::sync::mpsc;
    use tokio01_test::task::MockTask;

    #[tokio::test]
    async fn drop_when_full() {
        future::lazy(|cx| {
            let (tx, rx) = mpsc::channel(3);

            let mut tx = Box::pin(DropWhenFull::new(BoundedSink::new(tx)));

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
        let task = Arc::new(AtomicTask::new());
        let acker = Acker::Disk(counter, Arc::clone(&task));

        let mut mock = MockTask::new();

        mock.enter(|| task.register());

        assert!(!mock.is_notified());
        acker.ack(0);
        assert!(!mock.is_notified());
        acker.ack(1);
        assert!(mock.is_notified());
    }

    #[test]
    fn config_default_values() {
        fn check(source: &str, config: BufferConfig) {
            let conf: BufferConfig = toml::from_str(source).unwrap();
            assert_eq!(toml::to_string(&conf), toml::to_string(&config));
        }

        check(
            r#"
          type = "memory"
          "#,
            BufferConfig::Memory {
                max_events: 500,
                when_full: WhenFull::Block,
            },
        );

        check(
            r#"
          type = "memory"
          max_events = 100
          "#,
            BufferConfig::Memory {
                max_events: 100,
                when_full: WhenFull::Block,
            },
        );

        check(
            r#"
          type = "memory"
          when_full = "drop_newest"
          "#,
            BufferConfig::Memory {
                max_events: 500,
                when_full: WhenFull::DropNewest,
            },
        );

        #[cfg(feature = "leveldb")]
        check(
            r#"
          type = "disk"
          max_size = 1024
          "#,
            BufferConfig::Disk {
                max_size: 1024,
                when_full: WhenFull::Block,
            },
        );
    }
}
