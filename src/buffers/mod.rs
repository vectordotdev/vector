use crate::Event;
use futures01::{sync::mpsc, task::AtomicTask, AsyncSink, Poll, Sink, StartSend, Stream};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[cfg(feature = "leveldb")]
pub mod disk;

#[derive(Deserialize, Serialize, Debug)]
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

pub enum BufferInputCloner {
    Memory(mpsc::Sender<Event>, WhenFull),
    #[cfg(feature = "leveldb")]
    Disk(disk::Writer, WhenFull),
}

impl BufferInputCloner {
    pub fn get(&self) -> Box<dyn Sink<SinkItem = Event, SinkError = ()> + Send> {
        match self {
            BufferInputCloner::Memory(tx, when_full) => {
                let inner = tx.clone().sink_map_err(|e| error!("sender error: {:?}", e));
                if when_full == &WhenFull::DropNewest {
                    Box::new(DropWhenFull { inner })
                } else {
                    Box::new(inner)
                }
            }

            #[cfg(feature = "leveldb")]
            BufferInputCloner::Disk(writer, when_full) => {
                if when_full == &WhenFull::DropNewest {
                    Box::new(DropWhenFull {
                        inner: writer.clone(),
                    })
                } else {
                    Box::new(writer.clone())
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
            Box<dyn Stream<Item = Event, Error = ()> + Send>,
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
                    .map_err(|err| err.to_string())?;
                let tx = BufferInputCloner::Disk(tx, *when_full);
                let rx = Box::new(rx);
                Ok((tx, rx, acker))
            }
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
        }
    }

    pub fn new_for_testing() -> (Self, Arc<AtomicUsize>) {
        let ack_counter = Arc::new(AtomicUsize::new(0));
        let notifier = Arc::new(AtomicTask::new());
        let acker = Acker::Disk(Arc::clone(&ack_counter), Arc::clone(&notifier));

        (acker, ack_counter)
    }
}

pub struct DropWhenFull<S> {
    inner: S,
}

impl<S: Sink> Sink for DropWhenFull<S> {
    type SinkItem = S::SinkItem;
    type SinkError = S::SinkError;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match self.inner.start_send(item) {
            Ok(AsyncSink::NotReady(_)) => {
                debug!(
                    message = "Shedding load; dropping event.",
                    rate_limit_secs = 10
                );
                Ok(AsyncSink::Ready)
            }
            other => other,
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.inner.poll_complete()
    }
}

#[cfg(test)]
mod test {
    use super::{Acker, BufferConfig, DropWhenFull, WhenFull};
    use crate::test_util::block_on;
    use futures01::{future, sync::mpsc, task::AtomicTask, Async, AsyncSink, Sink, Stream};
    use std::sync::{atomic::AtomicUsize, Arc};
    use tokio01_test::task::MockTask;

    #[test]
    fn drop_when_full() {
        block_on::<_, _, ()>(future::lazy(|| {
            let (tx, mut rx) = mpsc::channel(2);

            let mut tx = DropWhenFull { inner: tx };

            assert_eq!(tx.start_send(1), Ok(AsyncSink::Ready));
            assert_eq!(tx.start_send(2), Ok(AsyncSink::Ready));
            assert_eq!(tx.start_send(3), Ok(AsyncSink::Ready));
            assert_eq!(tx.start_send(4), Ok(AsyncSink::Ready));

            assert_eq!(rx.poll(), Ok(Async::Ready(Some(1))));
            assert_eq!(rx.poll(), Ok(Async::Ready(Some(2))));
            assert_eq!(rx.poll(), Ok(Async::Ready(Some(3))));
            assert_eq!(rx.poll(), Ok(Async::NotReady));

            future::ok(())
        }))
        .unwrap();
    }

    #[test]
    fn ack_with_none() {
        let counter = Arc::new(AtomicUsize::new(0));
        let task = Arc::new(AtomicTask::new());
        let acker = Acker::Disk(counter, task.clone());

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
