use crate::record::Record;
use futures::{sync::mpsc, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
        ),
        String,
    > {
        match self {
            BufferConfig::Memory { num_items } => {
                let (tx, rx) = mpsc::channel(*num_items);
                let tx = BufferInputCloner::Memory(tx);
                let rx = Box::new(rx);
                Ok((tx, rx))
            }
            #[cfg(feature = "leveldb")]
            BufferConfig::Disk { max_size } => {
                let path = data_dir
                    .as_ref()
                    .ok_or_else(|| "Must set data_dir to use on-disk buffering.".to_string())?
                    .join(format!("{}_buffer", sink_name));

                let (tx, rx) = disk::open(&path, *max_size);
                let tx = BufferInputCloner::Disk(tx);
                let rx = Box::new(rx);
                Ok((tx, rx))
            }
        }
    }
}
