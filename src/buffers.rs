use crate::record::Record;
use futures::{sync::mpsc, task::AtomicTask, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[cfg(feature = "leveldb")]
mod disk;

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "type")]
pub enum BufferConfig {
    Memory {
        num_items: usize,
    },
    #[cfg(feature = "leveldb")]
    Disk {
        max_size: usize,
    },
}

impl Default for BufferConfig {
    fn default() -> Self {
        BufferConfig::Memory { num_items: 100 }
    }
}

pub enum BufferInputCloner {
    Memory(mpsc::Sender<Record>),
    #[cfg(feature = "leveldb")]
    Disk(disk::Writer),
}

impl BufferInputCloner {
    pub fn get(&self) -> Box<dyn Sink<SinkItem = Record, SinkError = ()> + Send> {
        match self {
            BufferInputCloner::Memory(tx) => {
                Box::new(tx.clone().sink_map_err(|e| error!("sender error: {:?}", e)))
            }
            #[cfg(feature = "leveldb")]
            BufferInputCloner::Disk(writer) => Box::new(writer.clone()),
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
        match self {
            BufferConfig::Memory { num_items } => {
                let (tx, rx) = mpsc::channel(*num_items);
                let tx = BufferInputCloner::Memory(tx);
                let rx = Box::new(rx);
                Ok((tx, rx, Acker::Null))
            }
            #[cfg(feature = "leveldb")]
            BufferConfig::Disk { max_size } => {
                let path = data_dir
                    .as_ref()
                    .ok_or_else(|| "Must set data_dir to use on-disk buffering.".to_string())?
                    .join(format!("{}_buffer", sink_name));

                let (tx, rx, acker) = disk::open(&path, *max_size);
                let tx = BufferInputCloner::Disk(tx);
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
