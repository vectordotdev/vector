#[cfg(feature = "disk-buffer")]
use std::path::PathBuf;
use std::{
    io::copy,
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use buffers::{
    bytes::{DecodeBytes, EncodeBytes},
    Variant, WhenFull,
};
use bytes::{Buf, BufMut};
use core_common::byte_size_of::ByteSizeOf;
use futures::{SinkExt, StreamExt};
use hdrhistogram::Histogram;
use human_bytes::human_bytes;
use lading_common::{
    block::{chunk_bytes, construct_block_cache, Block},
    payload,
};
use tokio::{select, sync::oneshot, time};
use tracing::Span;

#[derive(Clone, Debug)]
struct RecordWrapper(Vec<u8>);

impl ByteSizeOf for RecordWrapper {
    fn allocated_bytes(&self) -> usize {
        self.0.capacity()
    }
}

impl EncodeBytes<RecordWrapper> for RecordWrapper {
    type Error = ();

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::Error>
    where
        B: BufMut,
    {
        buffer.put_slice(self.0.as_slice());
        Ok(())
    }
}

impl DecodeBytes<RecordWrapper> for RecordWrapper {
    type Error = String;

    fn decode<B>(buffer: B) -> Result<RecordWrapper, Self::Error>
    where
        B: Buf,
    {
        let buf_len = buffer.remaining();
        let mut writer = Vec::with_capacity(buf_len);
        let mut reader = buffer.reader();
        let n = copy(&mut reader, &mut writer).expect("should never fail to copy");
        assert_eq!(n, buf_len as u64);

        Ok(RecordWrapper(writer))
    }
}

fn generate_block_cache() -> Vec<Block> {
    // Generate a 256MB block cache of small JSON messages.  This gives us pre-built messages that
    // are slightly varied in size, and differing in contained data, up to 256MB total.  Now we can
    // better simulate actual transactions with the speed of pre-computing the records.
    let mut rng = rand::thread_rng();
    let labels = vec![("target".to_string(), "disk_v1".to_string())];
    let block_chunks = chunk_bytes(&mut rng, 256_000_000, &[512, 768, 1024, 2048]);
    construct_block_cache(&payload::Json::default(), &block_chunks, &labels)
}

#[tokio::main]
async fn main() {
    // Generate our block cache, which ensures the writer spends as little time as possible actually
    // generating the data that it writes to the buffer.
    let block_cache = generate_block_cache();
    println!(
        "[disk_v1 init] generated block cache of 256MB ({} blocks)",
        block_cache.len()
    );

    // Set up our target record count, write batch size, progress counters, etc.
    let total_records = 3_000_000;

    let write_position = Arc::new(AtomicUsize::new(0));
    let read_position = Arc::new(AtomicUsize::new(0));
    let bytes_written = Arc::new(AtomicUsize::new(0));
    let bytes_read = Arc::new(AtomicUsize::new(0));

    let writer_position = Arc::clone(&write_position);
    let reader_position = Arc::clone(&read_position);
    let writer_bytes_written = Arc::clone(&bytes_written);
    let reader_bytes_read = Arc::clone(&bytes_read);

    // Now create the writer and reader and their associated tasks.
    let start = Instant::now();
    let variant = Variant::Disk {
        id: "disk-v1".to_owned(),
        data_dir: PathBuf::from_str("/tmp/vector/disk-v1-testing").expect("path should be valid"),
        // We don't limit disk size, because we just want to see how fast it can complete the writes/reads.
        max_size: usize::MAX,
        when_full: WhenFull::Block,
    };

    let (writer, mut reader, acker) =
        buffers::build::<RecordWrapper>(variant, Span::none()).expect("failed to create buffer");
    let mut writer = writer.get();

    let (reader_tx, mut reader_rx) = oneshot::channel();
    let _ = tokio::spawn(async move {
        let mut rx_histo = Histogram::<u64>::new(3).expect("should not fail");

        let mut unacked = 0;
        for _ in 0..total_records {
            let rx_start = Instant::now();

            let record_buf = reader.next().await.expect("read should not fail");
            unacked += 1;
            if unacked >= 1000 {
                acker.ack(unacked);
                unacked = 0;
            }

            let elapsed = rx_start.elapsed().as_nanos() as u64;
            rx_histo.record(elapsed).expect("should not fail");

            reader_position.fetch_add(1, Ordering::Relaxed);
            reader_bytes_read.fetch_add(record_buf.0.len(), Ordering::Relaxed);
        }

        let _ = reader_tx.send((reader, rx_histo));
    });

    let (writer_tx, mut writer_rx) = oneshot::channel();
    let _ = tokio::spawn(async move {
        let mut tx_histo = Histogram::<u64>::new(3).expect("should not fail");
        let mut records = block_cache.iter().cycle();

        for _ in 0..total_records {
            let tx_start = Instant::now();

            let record = records.next().expect("should never be empty");
            let tx_bytes = record.bytes.len();
            if let Err(_) = writer.send(RecordWrapper(record.bytes.clone())).await {
                panic!("failed to write record");
            }

            let elapsed = tx_start.elapsed().as_nanos() as u64;
            tx_histo.record(elapsed).expect("should not fail");

            writer_position.fetch_add(1, Ordering::Relaxed);
            writer_bytes_written.fetch_add(tx_bytes, Ordering::Relaxed);
        }

        let _ = writer_tx.send((writer, tx_histo));
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
                    println!("[disk_v1 writer] {:?}: finished", start.elapsed());
                },
                Err(_) => panic!("[disk_v1 writer] task failed unexpectedly!"),
            },
            result = &mut reader_rx, if reader_result.is_none() => match result {
                Ok(result) => {
                    reader_result = Some(result);
                    println!("[disk_v1 reader] {:?}: finished", start.elapsed());
                },
                Err(_) => panic!("[disk_v1 reader] task failed unexpectedly!"),
            },
            _ = progress_interval.tick(), if writer_result.is_none() || reader_result.is_none() => {
                let elapsed = start.elapsed();
                let write_pos = write_position.load(Ordering::Relaxed);
                let read_pos = read_position.load(Ordering::Relaxed);
                let write_bytes = bytes_written.load(Ordering::Relaxed);
                let read_bytes = bytes_read.load(Ordering::Relaxed);

                println!("[disk_v1 writer] {:?}s: position = {:11}, bytes wrtn = {:11}", elapsed.as_secs(), write_pos, write_bytes);
                println!("[disk_v1 reader] {:?}s: position = {:11}, bytes read = {:11}", elapsed.as_secs(), read_pos, read_bytes);
            },
            else => break,
        }
    }

    // Now dump out all of our summary statistics.
    let total_time = start.elapsed();
    let write_bytes = bytes_written.load(Ordering::Relaxed);
    let read_bytes = bytes_read.load(Ordering::Relaxed);
    assert_eq!(write_bytes, read_bytes);

    println!(
        "[disk_v1] writer and reader done: {} total records ({}) written and read in {:?}",
        total_records,
        human_bytes(write_bytes as f64),
        total_time
    );

    println!("[disk_v1] writer summary:");

    let (_, writer_histo) = writer_result.unwrap();
    let rps = write_position.load(Ordering::Relaxed) as f64 / total_time.as_secs_f64();

    println!("  -> records per second: {}", rps as u64);
    println!("  -> tx latency histo:");
    println!("       q=min -> {:?}", nanos_to_dur(writer_histo.min()));
    for q in &[0.5, 0.95, 0.99, 0.999, 0.9999] {
        let latency = writer_histo.value_at_quantile(*q);
        println!("       q={} -> {:?}", q, nanos_to_dur(latency));
    }
    println!("       q=max -> {:?}", nanos_to_dur(writer_histo.max()));

    println!("[disk_v1] reader summary:");

    let (_, reader_histo) = reader_result.unwrap();
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
