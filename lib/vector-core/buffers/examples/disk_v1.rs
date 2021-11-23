#[cfg(feature = "disk-buffer")]
use std::path::PathBuf;
use std::{
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use buffers::{helpers::VariableMessage, Variant, WhenFull};
use futures::{SinkExt, StreamExt};
use hdrhistogram::Histogram;
use rand::Rng;
use tokio::{select, sync::oneshot, time};
use tracing::Span;

fn generate_record_cache() -> Vec<VariableMessage> {
    // Generate a bunch of `VariableMessage` records that we'll cycle through, with payloads between
    // 512 bytes and 8 kilobytes.  This shiuld be fairly close to normal log events.
    let mut rng = rand::thread_rng();
    let mut records = Vec::new();
    for i in 0..200_000 {
        let payload_size = rng.gen_range(512..4096);
        let payload = (0..payload_size).map(|_| rng.gen()).collect();
        let message = VariableMessage::new(i, payload);
        records.push(message);
    }
    records
}

#[tokio::main]
async fn main() {
    // Generate our block cache, which ensures the writer spends as little time as possible actually
    // generating the data that it writes to the buffer.
    let record_cache = generate_record_cache();
    println!(
        "[disk_v1 init] generated record cache ({} blocks)",
        record_cache.len()
    );

    // Set up our target record count, write batch size, progress counters, etc.
    let total_records = 3_000_000;

    let write_position = Arc::new(AtomicUsize::new(0));
    let read_position = Arc::new(AtomicUsize::new(0));

    let writer_position = Arc::clone(&write_position);
    let reader_position = Arc::clone(&read_position);

    // Now create the writer and reader and their associated tasks.
    let start = Instant::now();
    let variant = Variant::Disk {
        id: "disk-v1".to_owned(),
        data_dir: PathBuf::from_str("/tmp/vector/disk-v1-testing").expect("path should be valid"),
        // We don't limit disk size, because we just want to see how fast it can complete the writes/reads.
        max_size: 5 * 1024 * 1024 * 1024,
        when_full: WhenFull::Block,
    };

    let (writer, mut reader, acker) =
        buffers::build::<VariableMessage>(variant, Span::none()).expect("failed to create buffer");
    let mut writer = writer.get();

    let (reader_tx, mut reader_rx) = oneshot::channel();
    let _ = tokio::spawn(async move {
        let mut rx_histo = Histogram::<u64>::new(3).expect("should not fail");

        let mut unacked = 0;
        for _ in 0..total_records {
            let rx_start = Instant::now();

            let _record = reader.next().await.expect("read should not fail");
            unacked += 1;
            if unacked >= 1000 {
                acker.ack(unacked);
                unacked = 0;
            }

            let elapsed = rx_start.elapsed().as_nanos() as u64;
            rx_histo.record(elapsed).expect("should not fail");

            reader_position.fetch_add(1, Ordering::Relaxed);
        }

        let _ = reader_tx.send((reader, rx_histo));
    });

    let (writer_tx, mut writer_rx) = oneshot::channel();
    let _ = tokio::spawn(async move {
        let mut tx_histo = Histogram::<u64>::new(3).expect("should not fail");
        let mut records = record_cache.iter().cycle();

        for _ in 0..total_records {
            let tx_start = Instant::now();

            let record = records.next().cloned().expect("should never be empty");
            if let Err(_) = writer.send(record).await {
                panic!("failed to write record");
            }

            let elapsed = tx_start.elapsed().as_nanos() as u64;
            tx_histo.record(elapsed).expect("should not fail");

            writer_position.fetch_add(1, Ordering::Relaxed);
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

                println!("[disk_v1 writer] {:?}s: position = {:11}", elapsed.as_secs(), write_pos);
                println!("[disk_v1 reader] {:?}s: position = {:11}", elapsed.as_secs(), read_pos);
            },
            else => break,
        }
    }

    // Now dump out all of our summary statistics.
    let total_time = start.elapsed();

    println!(
        "[disk_v1] writer and reader done: {} total records written and read in {:?}",
        total_records, total_time
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
