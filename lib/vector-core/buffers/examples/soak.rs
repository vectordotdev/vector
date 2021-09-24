use std::{env, fmt, path::PathBuf, process, time::Duration};

use buffers::{
    bytes::{DecodeBytes, EncodeBytes},
    Variant, WhenFull,
};
use bytes::{Buf, BufMut};
use core_common::byte_size_of::ByteSizeOf;
use futures::{SinkExt, StreamExt};
use metrics::{counter, increment_counter};
use metrics_exporter_prometheus::PrometheusBuilder;
use tokio::{
    pin, select,
    time::{interval, sleep},
};

#[derive(Clone, Copy)]
pub struct Message<const N: usize> {
    id: u64,
    _padding: [u64; N],
}

impl<const N: usize> Message<N> {
    fn new(id: u64) -> Self {
        Message {
            id,
            _padding: [0; N],
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

impl<const N: usize> ByteSizeOf for Message<N> {
    fn allocated_bytes(&self) -> usize {
        self.id.size_of() + self._padding.iter().fold(0, |acc, v| acc + v.size_of())
    }
}

#[derive(Debug)]
pub enum EncodeError {}

#[derive(Debug)]
pub enum DecodeError {}

impl fmt::Display for DecodeError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}

impl<const N: usize> EncodeBytes<Message<N>> for Message<N> {
    type Error = EncodeError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::Error>
    where
        B: BufMut,
        Self: Sized,
    {
        buffer.put_u64(self.id);
        for _ in 0..N {
            // this covers self._padding
            buffer.put_u64(0);
        }
        Ok(())
    }
}

impl<const N: usize> DecodeBytes<Message<N>> for Message<N> {
    type Error = DecodeError;

    fn decode<B>(mut buffer: B) -> Result<Self, Self::Error>
    where
        B: Buf,
        Self: Sized,
    {
        let id = buffer.get_u64();
        for _ in 0..N {
            // this covers self._padding
            let _ = buffer.get_u64();
        }
        Ok(Message::new(id))
    }
}

#[tokio::main]
async fn main() {
    let mut cli_args = env::args().collect::<Vec<_>>();
    if cli_args.len() != 3 {
        eprintln!("Usage: soak <data dir> <database size in bytes>");
        process::exit(1);
    }

    let _ = PrometheusBuilder::new()
        .install()
        .expect("exporter install should not fail");

    let _ = cli_args.remove(0);
    let data_dir: PathBuf = cli_args
        .remove(0)
        .parse()
        .expect("database path must be a valid path");
    let db_size: usize = cli_args
        .remove(0)
        .parse()
        .expect("database size must be a non-negative amount");
    let variant = Variant::Disk {
        id: "debug".to_owned(),
        data_dir,
        max_size: db_size,
        when_full: WhenFull::DropNewest,
    };

    let (writer, reader, acker) = buffers::build(variant).expect("failed to create buffer");
    let _ = tokio::spawn(async move {
        let mut id = 0;
        let mut writer = writer.get();
        loop {
            let msg = Message::<64>::new(id);
            id += 1;
            if let Err(e) = writer.send(msg).await {
                eprintln!("writer error: {:?}", e);
                increment_counter!("buffers_soak_msgs_written_error");
            } else {
                increment_counter!("buffers_soak_msgs_written_success");
            }
        }
    });
    let _ = tokio::spawn(async move {
        pin!(reader);

        let mut unacked = 0;
        let mut highest_id_seen = 0;
        let ack_timeout = interval(Duration::from_secs(5));
        pin!(ack_timeout);

        loop {
            select! {
                _ = ack_timeout.tick() => {
                    counter!("buffers_soak_msgs_acked", unacked as u64);
                    acker.ack(unacked);
                    unacked = 0;
                },
                Some(msg) = reader.next() => {
                    if msg.id() <= highest_id_seen && msg.id() != 0 {
                        eprintln!("out-of-order reader: got {}, but highest ID seen is {}", msg.id(), highest_id_seen);
                    } else {
                        highest_id_seen = msg.id();
                    }
                    unacked += 1;
                    increment_counter!("buffers_soak_msgs_read");
                },
                else => {
                    println!("reader stream terminated");
                    break
                }
            }
        }
    });

    sleep(Duration::from_secs(24 * 60 * 60)).await;
}
