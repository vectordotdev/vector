use std::{
    cmp, error, fmt,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use bytes::{Buf, BufMut};
use clap::{Arg, Command};
use hdrhistogram::Histogram;
use rand::Rng;
use tokio::{select, sync::oneshot, task, time};
use tracing::{debug, info, Span};
use tracing_subscriber::EnvFilter;
use vector_buffers::{
    encoding::FixedEncodable,
    topology::{
        builder::TopologyBuilder,
        channel::{BufferReceiver, BufferSender},
    },
    BufferType, Bufferable, EventCount, WhenFull,
};
use vector_common::byte_size_of::ByteSizeOf;
use vector_common::finalization::{
    AddBatchNotifier, BatchNotifier, EventFinalizer, EventFinalizers, EventStatus, Finalizable,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableMessage {
    id: u64,
    payload: Vec<u8>,
    finalizers: EventFinalizers,
}

impl VariableMessage {
    pub fn new(id: u64, payload: Vec<u8>) -> Self {
        VariableMessage {
            id,
            payload,
            finalizers: Default::default(),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

impl AddBatchNotifier for VariableMessage {
    fn add_batch_notifier(&mut self, batch: BatchNotifier) {
        self.finalizers.add(EventFinalizer::new(batch));
    }
}

impl ByteSizeOf for VariableMessage {
    fn allocated_bytes(&self) -> usize {
        self.payload.len()
    }
}

impl EventCount for VariableMessage {
    fn event_count(&self) -> usize {
        1
    }
}

impl Finalizable for VariableMessage {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl FixedEncodable for VariableMessage {
    type EncodeError = EncodeError;
    type DecodeError = DecodeError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::EncodeError>
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

    fn decode<B>(mut buffer: B) -> Result<Self, Self::DecodeError>
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

struct Configuration {
    buffer_type: String,
    read_total_records: usize,
    write_total_records: usize,
    write_batch_size: usize,
    min_record_size: usize,
    max_record_size: usize,
}

impl Configuration {
    pub fn from_cli() -> Result<Self, String> {
        let matches = Command::new("buffer-perf")
            .about("Runner for performance testing of buffers")
            .arg(
                Arg::new("buffer_type")
                    .help("Sets the buffer type to use")
                    .short('t')
                    .long("buffer-type")
                    .value_parser(["disk-v1", "disk-v2", "in-memory"])
                    .default_value("disk-v2"),
            )
            .arg(
                Arg::new("read_total_records")
                    .help("Sets the total number of records that should be read")
                    .long("read-total-records")
                    .default_value("10000000"),
            )
            .arg(
                Arg::new("write_total_records")
                    .help("Sets the total number of records that should be write")
                    .long("write-total-records")
                    .default_value("10000000"),
            )
            .arg(
                Arg::new("write_batch_size")
                    .help("Sets the batch size for writing")
                    .short('b')
                    .long("write-batch-size")
                    .default_value("100"),
            )
            .arg(
                Arg::new("min_record_size")
                    .help("Sets the lower bound of the size of the pre-generated records")
                    .long("min-record-size")
                    .default_value("512"),
            )
            .arg(
                Arg::new("max_record_size")
                    .help("Sets the upper bound of the size of the pre-generated records")
                    .long("max-record-size")
                    .default_value("4096"),
            )
            .get_matches();

        let buffer_type = matches
            .get_one::<String>("buffer_type")
            .map(|s| s.to_string())
            .expect("default value for buffer_type should always be present");
        let read_total_records = matches
            .get_one::<String>("read_total_records")
            .map(Ok)
            .expect("default value for read_total_records should always be present")
            .and_then(|s| s.parse::<usize>())
            .map_err(|e| e.to_string())?;
        let write_total_records = matches
            .get_one::<String>("write_total_records")
            .map(Ok)
            .expect("default value for write_total_records should always be present")
            .and_then(|s| s.parse::<usize>())
            .map_err(|e| e.to_string())?;
        let write_batch_size = matches
            .get_one::<String>("write_batch_size")
            .map(Ok)
            .expect("default value for write_batch_size should always be present")
            .and_then(|s| s.parse::<usize>())
            .map_err(|e| e.to_string())?;
        let min_record_size = matches
            .get_one::<String>("min_record_size")
            .map(Ok)
            .expect("default value for min_record_size should always be present")
            .and_then(|s| s.parse::<usize>())
            .map_err(|e| e.to_string())?;
        let max_record_size = matches
            .get_one::<String>("max_record_size")
            .map(Ok)
            .expect("default value for max_record_size should always be present")
            .and_then(|s| s.parse::<usize>())
            .map_err(|e| e.to_string())?;

        Ok(Configuration {
            buffer_type,
            read_total_records,
            write_total_records,
            write_batch_size,
            min_record_size,
            max_record_size,
        })
    }
}

fn generate_record_cache(min: usize, max: usize) -> Vec<VariableMessage> {
    let mut rng = rand::thread_rng();
    let mut records = Vec::new();
    for i in 1..=200_000 {
        let payload_size = rng.gen_range(min..max);
        let payload = (0..payload_size).map(|_| rng.gen::<u8>()).collect();
        let message = VariableMessage::new(i, payload);
        records.push(message);
    }
    records
}

async fn generate_buffer<T>(buffer_type: &str) -> (BufferSender<T>, BufferReceiver<T>)
where
    T: Bufferable + Clone + Finalizable,
{
    let data_dir = PathBuf::from("/tmp/vector");
    let id = format!("{}-buffer-perf-testing", buffer_type);
    let max_size_events = std::num::NonZeroUsize::new(500).unwrap();
    let max_size_bytes = std::num::NonZeroU64::new(32 * 1024 * 1024 * 1024).unwrap();
    let when_full = WhenFull::Block;

    let mut builder = TopologyBuilder::default();

    let variant = match buffer_type {
        "in-memory" => {
            info!(
                "[buffer-perf] creating in-memory v2 buffer with max_events={}, in blocking mode",
                max_size_events
            );
            BufferType::Memory {
                max_events: max_size_events,
                when_full,
            }
        }
        "disk-v2" => {
            info!(
                "[buffer-perf] creating disk v2 buffer with max_size={}, in blocking mode",
                max_size_bytes
            );
            BufferType::DiskV2 {
                max_size: max_size_bytes,
                when_full,
            }
        }
        s => panic!(
            "unknown buffer type '{}' requested; valid types are in-memory, disk-v1, and disk-v2",
            s
        ),
    };

    variant
        .add_to_builder(&mut builder, Some(data_dir), id)
        .expect("should not fail to add variant to builder");

    builder
        .build(String::from("buffer_perf"), Span::none())
        .await
        .expect("build should not fail")
}

#[tokio::main(flavor = "multi_thread", worker_threads = 16)]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Configuration::from_cli().expect("reading config parameters failed");

    let read_total_records = config.read_total_records;
    let write_total_records = config.write_total_records;
    let write_batch_size = config.write_batch_size;
    debug!(
        "[buffer-perf] going to write {} records, with a write batch size of {} record(s), and read {} records",
        write_total_records, write_batch_size, read_total_records
    );

    let record_cache = generate_record_cache(config.min_record_size, config.max_record_size);
    info!(
        "[buffer-perf] generated record cache ({} records, {}-{} bytes)",
        record_cache.len(),
        config.min_record_size,
        config.max_record_size
    );

    let write_position = Arc::new(AtomicUsize::new(0));
    let read_position = Arc::new(AtomicUsize::new(0));

    let writer_position = Arc::clone(&write_position);
    let reader_position = Arc::clone(&read_position);

    let start = Instant::now();
    info!(
        "[buffer-perf] {:?}s: creating buffer...",
        start.elapsed().as_secs()
    );

    let buffer_start = Instant::now();
    let (mut writer, mut reader) = generate_buffer(config.buffer_type.as_str()).await;
    let buffer_delta = buffer_start.elapsed();

    info!(
        "[buffer-perf] {:?}s: created/loaded buffer in {:?}",
        start.elapsed().as_secs(),
        buffer_delta
    );

    let (writer_tx, mut writer_rx) = oneshot::channel();
    let writer_task = async move {
        let tx_start = Instant::now();

        let mut tx_histo = Histogram::<u64>::new(3).expect("should not fail");
        let mut records = record_cache.iter().cycle().cloned();

        let mut remaining = write_total_records;
        while remaining > 0 {
            let write_start = Instant::now();

            let records_written = match write_batch_size {
                0 => unreachable!(),
                1 => {
                    let record = records.next().expect("should never be empty");
                    writer
                        .send(record, None)
                        .await
                        .expect("failed to write record");
                    1
                }
                n => {
                    let count = cmp::min(n, remaining);
                    let record_chunk = (&mut records).take(count);
                    for record in record_chunk {
                        writer
                            .send(record, None)
                            .await
                            .expect("failed to write record");
                    }
                    count
                }
            };

            remaining -= records_written;

            task::yield_now().await;

            writer.flush().await.expect("flush should not fail");

            let elapsed = write_start.elapsed().as_nanos() as u64;
            tx_histo.record(elapsed).expect("should not fail");

            writer_position.fetch_add(records_written, Ordering::Relaxed);
        }

        writer.flush().await.expect("flush shouldn't fail");
        let total_tx_dur = tx_start.elapsed();

        writer_tx
            .send((total_tx_dur, tx_histo))
            .expect("should not fail");
    };
    tokio::spawn(writer_task);

    let (reader_tx, mut reader_rx) = oneshot::channel();
    let reader_task = async move {
        let rx_start = Instant::now();

        let mut rx_histo = Histogram::<u64>::new(3).expect("should not fail");

        for _ in 0..read_total_records {
            let read_start = Instant::now();

            match reader.next().await {
                Some(mut record) => record
                    .take_finalizers()
                    .update_status(EventStatus::Delivered),
                None => {
                    info!("[buffer-perf] reader hit end of buffer, closing...");
                    break;
                }
            }

            let elapsed = read_start.elapsed().as_nanos() as u64;
            rx_histo.record(elapsed).expect("should not fail");

            reader_position.fetch_add(1, Ordering::Relaxed);
        }
        let total_rx_dur = rx_start.elapsed();

        reader_tx
            .send((total_rx_dur, rx_histo))
            .expect("should not fail");
    };
    tokio::spawn(reader_task);

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
                    info!("[buffer-perf] {:?}s: writer finished", start.elapsed().as_secs());
                },
                Err(_) => panic!("[buffer-perf] writer task failed unexpectedly!"),
            },
            result = &mut reader_rx, if reader_result.is_none() => match result {
                Ok(result) => {
                    reader_result = Some(result);
                    info!("[buffer-perf] {:?}s: reader finished", start.elapsed().as_secs());
                },
                Err(_) => panic!("[buffer-perf] reader task failed unexpectedly!"),
            },
            _ = progress_interval.tick(), if writer_result.is_none() || reader_result.is_none() => {
                let elapsed = start.elapsed();
                let write_pos = write_position.load(Ordering::Relaxed);
                let read_pos = read_position.load(Ordering::Relaxed);

                info!("[buffer-perf] {:?}s: writer pos = {:11}, reader pos = {:11}", elapsed.as_secs(), write_pos, read_pos);
            },
            else => break,
        }
    }

    // Now dump out all of our summary statistics.
    let total_time = start.elapsed();
    let read_pos = read_position.load(Ordering::Relaxed);
    let write_pos = write_position.load(Ordering::Relaxed);

    info!(
        "[buffer-perf] writer and reader done: {} records written and {} records read in {:?}",
        write_pos, read_pos, total_time
    );

    info!("[buffer-perf] writer summary:");

    let (writer_dur, writer_histo) = writer_result.unwrap();
    let write_rps = write_pos as f64 / writer_dur.as_secs_f64();

    info!("  -> records written per second: {}", write_rps as u64);
    info!("  -> tx latency histo:");
    info!("       q=min -> {:?}", nanos_to_dur(writer_histo.min()));
    for q in &[0.5, 0.95, 0.99, 0.999, 0.9999] {
        let latency = writer_histo.value_at_quantile(*q);
        info!("       q={} -> {:?}", q, nanos_to_dur(latency));
    }
    info!("       q=max -> {:?}", nanos_to_dur(writer_histo.max()));

    info!("[buffer-perf] reader summary:");

    let (reader_dur, reader_histo) = reader_result.unwrap();
    let read_rps = read_pos as f64 / reader_dur.as_secs_f64();

    info!("  -> records read per second: {}", read_rps as u64);
    info!("  -> rx latency histo:");
    info!("       q=min -> {:?}", nanos_to_dur(reader_histo.min()));
    for q in &[0.5, 0.95, 0.99, 0.999, 0.9999] {
        let latency = reader_histo.value_at_quantile(*q);
        info!("       q={} -> {:?}", q, nanos_to_dur(latency));
    }
    info!("       q=max -> {:?}", nanos_to_dur(reader_histo.max()));
}

fn nanos_to_dur(nanos: u64) -> Duration {
    Duration::from_nanos(nanos)
}
