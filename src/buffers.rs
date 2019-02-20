use crate::record::Record;
use futures::{sync::mpsc, Sink, Stream};
use log::error;
use serde_derive::{Deserialize, Serialize};
use std::path::PathBuf;

mod disk;

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "type")]
pub enum BufferConfig {
    Memory {
        num_items: usize,
    },
    Disk {
        // TODO: max_size
    },
}

impl Default for BufferConfig {
    fn default() -> Self {
        BufferConfig::Memory { num_items: 100 }
    }
}

pub enum BufferInputCloner {
    Memory(mpsc::Sender<Record>),
    Disk(disk::Writer),
}

impl BufferInputCloner {
    pub fn get(&self) -> Box<dyn Sink<SinkItem = Record, SinkError = ()> + Send> {
        match self {
            BufferInputCloner::Memory(tx) => {
                Box::new(tx.clone().sink_map_err(|e| error!("sender error: {:?}", e)))
            }
            BufferInputCloner::Disk(writer) => Box::new(writer.clone()),
        }
    }
}

impl BufferConfig {
    pub fn build(
        &self,
        data_dir: &Option<PathBuf>,
        sink_name: &str,
    ) -> Result<
        (
            BufferInputCloner,
            Box<dyn Stream<Item = Record, Error = ()> + Send>,
            mpsc::UnboundedSender<usize>,
        ),
        String,
    > {
        let (ack_tx, ack_rx) = mpsc::unbounded();

        match self {
            BufferConfig::Memory { num_items } => {
                let (tx, rx) = mpsc::channel(*num_items);
                let tx = BufferInputCloner::Memory(tx);
                let rx = Box::new(rx);
                Ok((tx, rx, ack_tx))
            }
            BufferConfig::Disk {} => {
                let path = data_dir
                    .as_ref()
                    .ok_or_else(|| "Must set data_dir to use on-disk buffering.".to_string())?
                    .join(format!("{}_buffer", sink_name));

                let (tx, rx) = disk::open(&path, ack_rx);
                let tx = BufferInputCloner::Disk(tx);
                let rx = Box::new(rx);
                Ok((tx, rx, ack_tx))
            }
        }
    }
}



use futures::{AsyncSink, Async};

pub struct SimpleAck<T: Sink<SinkItem=Record, SinkError=()>> {
    inner: T,
    ack_chan: mpsc::UnboundedSender<usize>,
    in_flight_ids: Vec<usize>,
}

impl Sink for SimpleAck {
    type SinkItem = (usize, Record);
    type SinkError = ();

    fn start_send(
        &mut self,
        (record_id, record): Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        match self.inner.start_send(record)? {
            AsyncSink::Ready => {
                self.in_flight_ids.push(record_id);
                Ok(AsyncSink::Ready)
            },
            AsyncSink::NotReady(record) => {
                Ok(AsyncSink::NotReady((record_id, record)))
            }
        }
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        let res = self.inner.poll_complete();

        if let Ok(Async::Ready(())) = res {
            for record_id in self.in_flight_ids.drain() {
                self.ack_send.unbounded_send(record_id);
            }
        }

        res
    }
}
