use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use buffers::{disk_v2::Buffer, helpers::VariableMessage};
use hdrhistogram::Histogram;
use human_bytes::human_bytes;
use rand::Rng;
use tokio::{select, sync::oneshot, time};

fn generate_record_cache() -> Vec<VariableMessage> {
    // Generate a bunch of `VariableMessage` records that we'll cycle through, with payloads between
    // 512 bytes and 8 kilobytes.  This shiuld be fairly close to normal log events.
    let mut rng = rand::thread_rng();
    let mut records = Vec::new();
    for i in 0..200_000 {
        let payload_size = rng.gen_range(512..8192);
        let payload = (0..payload_size).map(|_| rng.gen()).collect();
        let message = VariableMessage::new(i, payload);
        records.push(message);
    }
    records
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args();
    let writer_count = args
        .nth(1)
        .unwrap_or_else(|| "10000000".to_string())
        .parse::<usize>()
        .expect("1st arg must be number of records to write");
    let writer_batch_size = args
        .nth(0)
        .unwrap_or_else(|| "100".to_string())
        .parse::<usize>()
        .expect("2nd arg must be number of records per write batch");
    let reader_count: usize = args
        .nth(0)
        .unwrap_or_else(|| "10000000".to_string())
        .parse::<usize>()
        .expect("3rd arg must be number of records to read");

    println!(
        "[disk_v2 init] going to write {} record(s), in batches of {}, and will read {} record(s)",
        writer_count, writer_batch_size, reader_count
    );

    // Generate our record cache, which ensures the writer spends as little time as possible actually
    // generating the data that it writes to the buffer.
    let record_cache = generate_record_cache();
    println!(
        "[disk_v2 init] generated record cache ({} records)",
        record_cache.len()
    );

    // Set up our target record count, write batch size, progress counters, etc.
    let write_position = Arc::new(AtomicUsize::new(0));
    let read_position = Arc::new(AtomicUsize::new(0));
    let bytes_written = Arc::new(AtomicUsize::new(0));

    let writer_position = Arc::clone(&write_position);
    let reader_position = Arc::clone(&read_position);
    let writer_bytes_written = Arc::clone(&bytes_written);

    // Now create the writer and reader and their associated tasks.
    let start = Instant::now();
    let (mut writer, mut reader) = Buffer::from_path("/tmp/vector/disk-v2-testing")
        .await
        .expect("failed to open buffer");

    println!(
        "[disk_v2] startup ledger state: {}",
        writer.get_ledger_state()
    );

    let (writer_tx, mut writer_rx) = oneshot::channel();
    let _ = tokio::spawn(async move {
        let mut tx_histo = Histogram::<u64>::new(3).expect("should not fail");
        let mut records = record_cache.iter().cycle();

        let iters = writer_count / writer_batch_size;

        for _ in 0..iters {
            let tx_start = Instant::now();

            let mut tx_bytes_total = 0;
            for _ in 0..writer_batch_size {
                let record = records.next().cloned().expect("should never be empty");
                let n = writer
                    .write_record(record)
                    .await
                    .expect("failed to write record");
                tx_bytes_total += n;
            }
            writer.flush().await.expect("failed to flush writer");

            let elapsed = tx_start.elapsed().as_nanos() as u64;
            tx_histo.record(elapsed).expect("should not fail");

            writer_position.fetch_add(writer_batch_size, Ordering::Relaxed);
            writer_bytes_written.fetch_add(tx_bytes_total, Ordering::Relaxed);
        }

        let _ = writer_tx.send((writer, tx_histo));
    });

    let (reader_tx, mut reader_rx) = oneshot::channel();
    let _ = tokio::spawn(async move {
        let mut rx_histo = Histogram::<u64>::new(3).expect("should not fail");

        let mut last_id = None;
        for _ in 0..reader_count {
            let rx_start = Instant::now();

            let record = reader.next().await.expect("read should not fail");

            let elapsed = rx_start.elapsed().as_nanos() as u64;
            rx_histo.record(elapsed).expect("should not fail");

            if last_id.is_none() {
                println!("started reading at record ID {}", record.id());
            }
            last_id = Some(record.id());

            reader_position.fetch_add(1, Ordering::Relaxed);
        }

        if let Some(id) = last_id {
            println!("finished reading at record ID {}", id);
        }

        let _ = reader_tx.send((reader, rx_histo));
    });

    // Now let the tasks run, occasionally emitting metrics about their progress, while waiting for
    // them to complete.
    let mut progress_interval = time::interval(Duration::from_secs(1));
    let mut writer_result = None;
    let mut writer_elapsed = None;
    let mut reader_result = None;
    let mut reader_elapsed = None;

    let reader_writer_start = Instant::now();
    loop {
        select! {
            result = &mut writer_rx, if writer_result.is_none() => match result {
                Ok(result) => {
                    writer_result = Some(result);
                    writer_elapsed = Some(reader_writer_start.elapsed());
                    println!("[disk_v2 writer] {:?}: finished", start.elapsed());
                },
                Err(_) => panic!("[disk_v2 writer] task failed unexpectedly!"),
            },
            result = &mut reader_rx, if reader_result.is_none() => match result {
                Ok(result) => {
                    reader_result = Some(result);
                    reader_elapsed = Some(reader_writer_start.elapsed());
                    println!("[disk_v2 reader] {:?}: finished", start.elapsed());
                },
                Err(_) => panic!("[disk_v2 reader] task failed unexpectedly!"),
            },
            _ = progress_interval.tick(), if writer_result.is_none() || reader_result.is_none() => {
                //if let Some((writer, _)) = writer_result.as_mut() {
                //    writer.flush().await.expect("failed to flush");
                //}

                let elapsed = start.elapsed();
                let write_pos = write_position.load(Ordering::Relaxed);
                let read_pos = read_position.load(Ordering::Relaxed);
                let write_bytes = bytes_written.load(Ordering::Relaxed);

                println!("[disk_v2 writer] {:?}s: position = {:11}, bytes wrtn = {:11}", elapsed.as_secs(), write_pos, write_bytes);
                println!("[disk_v2 reader] {:?}s: position = {:11}", elapsed.as_secs(), read_pos);
            },
            else => break,
        }
    }

    // Now dump out all of our summary statistics.
    let total_time = start.elapsed();
    let write_bytes = bytes_written.load(Ordering::Relaxed);
    let writer_elapsed = writer_elapsed.expect("must be set if writer finished");
    let reader_elapsed = reader_elapsed.expect("must be set if reader finished");

    println!(
        "[disk_v2] writer and reader done: {} records written ({} bytes), {} records read, in {:?}",
        writer_count,
        human_bytes(write_bytes as f64),
        reader_count,
        total_time
    );

    let (writer, writer_histo) = writer_result.unwrap();
    println!(
        "[disk_v2] ending ledger state: {}",
        writer.get_ledger_state()
    );

    println!("[disk_v2] writer summary:");
    let write_rps = write_position.load(Ordering::Relaxed) as f64 / writer_elapsed.as_secs_f64();

    println!("  -> records per second: {}", write_rps as u64);
    println!("  -> tx latency histo:");
    println!("       q=min -> {:?}", nanos_to_dur(writer_histo.min()));
    for q in &[0.5, 0.95, 0.99, 0.999, 0.9999] {
        let latency = writer_histo.value_at_quantile(*q);
        println!("       q={} -> {:?}", q, nanos_to_dur(latency));
    }
    println!("       q=max -> {:?}", nanos_to_dur(writer_histo.max()));

    println!("[disk_v2] reader summary:");

    let (_, reader_histo) = reader_result.unwrap();
    let read_rps = read_position.load(Ordering::Relaxed) as f64 / reader_elapsed.as_secs_f64();

    println!("  -> records per second: {}", read_rps as u64);
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
