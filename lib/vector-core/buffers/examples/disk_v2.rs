use std::time::{Duration, Instant};

use buffers::disk_v2::Buffer;
use hdrhistogram::Histogram;
use lading_common::{
    block::{chunk_bytes, construct_block_cache},
    payload,
};

#[tokio::main]
async fn main() {
    let (mut writer, _reader) = Buffer::from_path("/tmp/mmap-testing")
        .await
        .expect("failed to open buffer");

    // Generate a 256MB block cache of small JSON messages.  This gives us pre-built messages that
    // are slightly varied in size, and differing in contained data, up to 256MB total.  Now we can
    // better simulate actual transactions with the speed of pre-computing the records.
    let mut rng = rand::thread_rng();
    let labels = vec![("target".to_string(), "disk_v2".to_string())];
    let block_chunks = chunk_bytes(&mut rng, 256_000_000, &[512, 768, 1024, 2048]);
    println!("generated {} block chunks", block_chunks.len());
    let block_cache = construct_block_cache(&payload::Json::default(), &block_chunks, &labels);
    let mut records = block_cache.iter().cycle();

    let start = Instant::now();
    let mut tx_histo = Histogram::<u64>::new(2).expect("should not fail");

    for i in 0..1_000_000 {
        let tx_start = Instant::now();
        let mut tx = writer.transaction();
        for _ in 0..100 {
            let record = records.next().expect("should never be empty");
            tx.write(&record.bytes)
                .await
                .expect("failed to write record");
        }
        tx.commit().await.expect("failed to commit transaction");
        let elapsed = tx_start.elapsed().as_nanos() as u64;

        tx_histo.record(elapsed).expect("should not fail");

        if i % 1000 == 0 {
            println!("{:?}: at txn {}", start.elapsed(), i);
        }
    }

    println!("summary:");

    let total_time = start.elapsed();
    let rps = writer.total_records() as f64 / total_time.as_secs_f64();

    println!("  -> total time: {:?}", total_time);
    println!("  -> records per second: {}", rps as u64);

    println!("  -> tx latency histo:");
    println!("       q=min -> {:?}", nanos_to_dur(tx_histo.min()));
    for q in &[0.25, 0.5, 0.9, 0.95, 0.99, 0.999] {
        let latency = tx_histo.value_at_quantile(*q);
        println!("       q={} -> {:?}", q, nanos_to_dur(latency));
    }
    println!("       q=max -> {:?}", nanos_to_dur(tx_histo.max()));
}

fn nanos_to_dur(nanos: u64) -> Duration {
    Duration::from_nanos(nanos)
}
