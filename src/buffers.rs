use crate::record::Record;
use futures::{sync::mpsc, task::AtomicTask, AsyncSink, Poll, Sink, StartSend, Stream};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[cfg(feature = "leveldb")]
mod disk;

#[derive(Deserialize, Serialize, Debug)]
pub struct BufferConfig {
    #[serde(flatten)]
    inner: BufferInnerConfig,
    when_full: WhenFull,
}

impl Default for BufferConfig {
    fn default() -> Self {
        BufferConfig {
            inner: Default::default(),
            when_full: WhenFull::Block,
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum BufferInnerConfig {
    Memory {
        num_items: usize,
    },
    #[cfg(feature = "leveldb")]
    Disk {
        max_size: usize,
    },
}

impl From<BufferInnerConfig> for BufferConfig {
    fn from(inner: BufferInnerConfig) -> Self {
        Self {
            inner,
            ..Default::default()
        }
    }
}

impl Default for BufferInnerConfig {
    fn default() -> Self {
        BufferInnerConfig::Memory { num_items: 100 }
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum WhenFull {
    Block,
    DropNewest,
}

pub enum BufferInputCloner {
    Memory(mpsc::Sender<Record>, WhenFull),
    #[cfg(feature = "leveldb")]
    Disk(disk::Writer, WhenFull),
}

impl BufferInputCloner {
    pub fn get(&self) -> Box<dyn Sink<SinkItem = Record, SinkError = ()> + Send> {
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
    #[cfg_attr(not(feature = "leveldb"), allow(unused))]
    pub fn build(
        &self,
        data_dir: &Option<PathBuf>,
        sink_name: &str,
    ) -> Result<
        (
            BufferInputCloner,
            Box<dyn Stream<Item = Record, Error = ()> + Send>,
            Acker,
        ),
        String,
    > {
        match &self.inner {
            BufferInnerConfig::Memory { num_items } => {
                let (tx, rx) = mpsc::channel(*num_items);
                let tx = BufferInputCloner::Memory(tx, self.when_full);
                let rx = Box::new(rx);
                Ok((tx, rx, Acker::Null))
            }
            #[cfg(feature = "leveldb")]
            BufferInnerConfig::Disk { max_size } => {
                let path = data_dir
                    .as_ref()
                    .ok_or_else(|| "Must set data_dir to use on-disk buffering.".to_string())?
                    .join(format!("{}_buffer", sink_name));

                let (tx, rx, acker) = disk::open(&path, *max_size);
                let tx = BufferInputCloner::Disk(tx, self.when_full);
                let rx = Box::new(rx);
                Ok((tx, rx, acker))
            }
        }
    }
}

pub enum Acker {
    Disk(Arc<AtomicUsize>, Arc<AtomicTask>),
    Null,
}

impl Acker {
    // This method should be called by a sink to indicate that it has successfully
    // flushed the next `num` records from its input stream. If there are records that
    // have flushed, but records that came before them in the stream have not been flushed,
    // the later records must _not_ be acked until all preceeding elements are also acked.
    // This is primary used by the on-disk buffer to know which records are okay to
    // delete from disk.
    pub fn ack(&self, num: usize) {
        match self {
            Acker::Null => {}
            Acker::Disk(counter, notifier) => {
                counter.fetch_add(num, Ordering::Relaxed);
                notifier.notify();
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
            Ok(AsyncSink::NotReady(_)) => Ok(AsyncSink::Ready),
            other => other,
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.inner.poll_complete()
    }
}

#[cfg(test)]
mod test {
    use super::DropWhenFull;
    use crate::test_util::block_on;
    use futures::{future, sync::mpsc, Async, AsyncSink, Sink, Stream};

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
}
