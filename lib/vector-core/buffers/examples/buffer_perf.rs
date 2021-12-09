#![allow(clippy::print_stderr)] // soak framework
#![allow(clippy::print_stdout)]
// Clippy allows are because this is an example/soak-y test, where we don't
// actually care about the presence of `println!` calls.
use std::path::PathBuf;
use std::{error, fmt};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use buffers::encoding::{DecodeBytes, EncodeBytes};
use buffers::topology::channel::{BufferReceiver, BufferSender};
use buffers::{topology::builder::TopologyBuilder, DiskV1Buffer, WhenFull};
use buffers::{Acker, Bufferable, DiskV2Buffer, MemoryBuffer};
use bytes::{Buf, BufMut};
use core_common::byte_size_of::ByteSizeOf;
use futures::{SinkExt, StreamExt};
use hdrhistogram::Histogram;
use rand::Rng;
use tokio::task;
use tokio::time::sleep;
use tokio::{select, sync::oneshot, time};
use tracing::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableMessage {
    id: u64,
    payload: Vec<u8>,
}

impl VariableMessage {
    pub fn new(id: u64, payload: Vec<u8>) -> Self {
        VariableMessage { id, payload }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

impl ByteSizeOf for VariableMessage {
    fn allocated_bytes(&self) -> usize {
        self.payload.len()
    }
}

impl EncodeBytes<VariableMessage> for VariableMessage {
    type Error = EncodeError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::Error>
    where
        B: BufMut,
        Self: Sized,
    {
        buffer.put_u64(self.id);
        buffer.put_u64(self.payload.len() as u64);
        buffer.put_slice(&self.payload);
        Ok(())
    }

    fn encoded_size(&self) -> Option<usize> {
        Some(8 + 8 + self.payload.len())
    }
}

impl DecodeBytes<VariableMessage> for VariableMessage {
    type Error = DecodeError;

    fn decode<B>(mut buffer: B) -> Result<Self, Self::Error>
    where
        B: Buf,
        Self: Sized,
    {
        let id = buffer.get_u64();

        // We only use `VariableMessage` in our example binaries, and we don't exceed that in
        // practice... it's just not important to worry about/
        #[allow(clippy::cast_possible_truncation)]
        let payload_len = buffer.get_u64() as usize;
        let payload = buffer.copy_to_bytes(payload_len).to_vec();
        Ok(VariableMessage::new(id, payload))
    }
}

#[derive(Debug)]
pub struct EncodeError;

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for EncodeError {}

#[derive(Debug)]
pub struct DecodeError;

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for DecodeError {}

fn generate_record_cache() -> Vec<VariableMessage> {
    // Generate a bunch of `VariableMessage` records that we'll cycle through, with payloads between
    // 512 bytes and 8 kilobytes.  This shiuld be fairly close to normal log events.
    let mut rng = rand::thread_rng();
    let mut records = Vec::new();
    for i in 1..200_000 {
        let payload_size = rng.gen_range(512..4096);
        let payload = (0..payload_size).map(|_| rng.gen()).collect();
        let message = VariableMessage::new(i, payload);
        records.push(message);
    }
    records
}

async fn generate_buffer<T>(buffer_type: &str) -> (BufferSender<T>, BufferReceiver<T>, Acker)
where
    T: Bufferable + Clone,
{
    let data_dir = PathBuf::from("/tmp/vector");
    let max_size_events = 500;
    let max_size_bytes = 32 * 1024 * 1024 * 1024;

    let mut builder = TopologyBuilder::new();

    match buffer_type {
        "in-memory" => {
            builder.stage(MemoryBuffer::new(max_size_events), WhenFull::Block);

            println!(
                "[buffer-perf] creating in-memory buffer with max_events={}, in blocking mode",
                max_size_events
            );
        }
        "disk-v1" => {
            let id = String::from("disk-v1-example");
            builder.stage(
                DiskV1Buffer::new(id, data_dir, max_size_bytes),
                WhenFull::Block,
            );

            println!(
                "[buffer-perf] creating disk v1 buffer with max_size={}, in blocking mode",
                max_size_bytes
            );
        }
        "disk-v2" => {
            let id = String::from("disk_v1_example");
            builder.stage(
                DiskV2Buffer::new(id, data_dir, max_size_bytes),
                WhenFull::Block,
            );

            println!(
                "[buffer-perf] creating disk v2 buffer with max_size={}, in blocking mode",
                max_size_bytes
            );
        }
        s => panic!(
            "unknown buffer type '{}' requested; valid types are in-memory, disk-v1, and disk-v2",
            s
        ),
    }

    builder
        .build(Span::none())
        .await
        .expect("build should not fail")
}

#[tokio::main(flavor = "multi_thread", worker_threads = 16)]
async fn main() {
    console_subscriber::init();

    let mut args = std::env::args();
    let buffer_type = args.nth(1).unwrap_or_else(|| "disk-v2".to_string());
    let writer_count = args
        .nth(0)
        .unwrap_or_else(|| "10000000".to_string())
        .parse::<usize>()
        .expect("writer count must be valid non-zero integer");
    let reader_count: usize = args
        .nth(0)
        .unwrap_or_else(|| "10000000".to_string())
        .parse::<usize>()
        .expect("reader count must be valid non-zero integer");
    let writer_batch_size: usize = args
        .nth(0)
        .unwrap_or_else(|| "1".to_string())
        .parse::<usize>()
        .expect("writer batch size must be valid non-zero integer");

    println!(
        "[buffer-perf] going to write {} record(s), read {} record(s), with a writer batch size of {} record(s)",
        writer_count, reader_count, writer_batch_size
    );

    // Generate our record cache, which ensures the writer spends as little time as possible actually
    // generating the data that it writes to the buffer.
    let record_cache = generate_record_cache();
    println!(
        "[buffer-perf] generated record cache ({} records)",
        record_cache.len()
    );

    let write_position = Arc::new(AtomicUsize::new(0));
    let read_position = Arc::new(AtomicUsize::new(0));

    let writer_position = Arc::clone(&write_position);
    let reader_position = Arc::clone(&read_position);

    // Create a disk buffer under /tmp/vector with the given ID and a maximum size of 32GB.
    let start = Instant::now();
    let (mut writer, mut reader, acker) = generate_buffer(buffer_type.as_str()).await;

    let (writer_tx, mut writer_rx) = oneshot::channel();
    tokio::spawn(async move {
        //sleep(Duration::from_secs(5)).await;

        let mut tx_histo = Histogram::<u64>::new(3).expect("should not fail");
        let mut records = record_cache.iter().cycle();

        let iters = writer_count / writer_batch_size;
        for _ in 0..iters {
            let tx_start = Instant::now();

            for _ in 0..writer_batch_size {
                let record = records.next().cloned().expect("should never be empty");
                writer.send(record).await.expect("failed to write record");

                //sleep(Duration::from_secs(1)).await;
            }

            task::yield_now().await;

            writer.flush().await.expect("flush should not fail");

            let elapsed = tx_start.elapsed().as_nanos() as u64;
            tx_histo.record(elapsed).expect("should not fail");

            writer_position.fetch_add(1, Ordering::Relaxed);
        }

        writer.flush().await.expect("flush shouldn't fail");
        writer_tx.send(tx_histo).expect("should not fail");
    });

    let (reader_tx, mut reader_rx) = oneshot::channel();
    tokio::spawn(async move {
        let mut rx_histo = Histogram::<u64>::new(3).expect("should not fail");

        for _ in 0..reader_count {
            let rx_start = Instant::now();

            let _record = reader.next().await.expect("read should not fail");
            acker.ack(1);

            let elapsed = rx_start.elapsed().as_nanos() as u64;
            rx_histo.record(elapsed).expect("should not fail");

            reader_position.fetch_add(1, Ordering::Relaxed);
        }

        reader_tx.send(rx_histo).expect("should not fail");
    });

    // Now let the tasks run, occasionally emitting metrics about their progress, while waiting for
    // them to complete.
    let mut progress_interval = time::interval(Duration::from_secs(1));
    let mut writer_result = None;
    let mut reader_result = None;

    loop {
        select! {
            result = &mut writer_rx, if writer_result.is_none() => match result {
                Ok(result) => {
                    writer_result = Some(result);
                    println!("[buffer-perf] (writer) {:?}: finished", start.elapsed());
                },
                Err(_) => panic!("[buffer-perf] (writer) task failed unexpectedly!"),
            },
            result = &mut reader_rx, if reader_result.is_none() => match result {
                Ok(result) => {
                    reader_result = Some(result);
                    println!("[buffer-perf] (reader) {:?}: finished", start.elapsed());
                },
                Err(_) => panic!("[buffer-perf] (reader) task failed unexpectedly!"),
            },
            _ = progress_interval.tick(), if writer_result.is_none() || reader_result.is_none() => {
                let elapsed = start.elapsed();
                let write_pos = write_position.load(Ordering::Relaxed);
                let read_pos = read_position.load(Ordering::Relaxed);

                println!("[buffer-perf] (writer) {:?}s: position = {:11}", elapsed.as_secs(), write_pos);
                println!("[buffer-perf] (reader) {:?}s: position = {:11}", elapsed.as_secs(), read_pos);
            },
            else => break,
        }
    }

    // Now dump out all of our summary statistics.
    let total_time = start.elapsed();

    println!(
        "[buffer-perf] writer and reader done: {} records written, {} records read, in {:?}",
        writer_count, reader_count, total_time
    );

    println!("[buffer-perf] writer summary:");

    let writer_histo = writer_result.unwrap();
    let rps = write_position.load(Ordering::Relaxed) as f64 / total_time.as_secs_f64();

    println!("  -> records per second: {}", rps as u64);
    println!("  -> tx latency histo:");
    println!("       q=min -> {:?}", nanos_to_dur(writer_histo.min()));
    for q in &[0.5, 0.95, 0.99, 0.999, 0.9999] {
        let latency = writer_histo.value_at_quantile(*q);
        println!("       q={} -> {:?}", q, nanos_to_dur(latency));
    }
    println!("       q=max -> {:?}", nanos_to_dur(writer_histo.max()));

    println!("[buffer-perf] reader summary:");

    let reader_histo = reader_result.unwrap();
    let rps = read_position.load(Ordering::Relaxed) as f64 / total_time.as_secs_f64();

    println!("  -> records per second: {}", rps as u64);
    println!("  -> rx latency histo:");
    println!("       q=min -> {:?}", nanos_to_dur(reader_histo.min()));
    for q in &[0.5, 0.95, 0.99, 0.999, 0.9999] {
        let latency = reader_histo.value_at_quantile(*q);
        println!("       q={} -> {:?}", q, nanos_to_dur(latency));
    }
    println!("       q=max -> {:?}", nanos_to_dur(reader_histo.max()));
}

fn nanos_to_dur(nanos: u64) -> Duration {
    Duration::from_nanos(nanos)
}
