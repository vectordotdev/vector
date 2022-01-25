use std::{mem, path::PathBuf, time::Duration};

use criterion::{
    criterion_group, criterion_main, measurement::WallTime, BatchSize, BenchmarkGroup, BenchmarkId,
    Criterion, SamplingMode, Throughput,
};
use tokio::runtime::{Handle, Runtime};
use vector_buffers::{BufferType, WhenFull};

use crate::common::{init_instrumentation, war_measurement, wtr_measurement};

mod common;

/// A struct to manage the data_dir of an on-disk benchmark
///
/// The way our benchmarks function we have the choice of sharing a data_dir
/// between benchmarks or give each benchmark its own data_dir. The first option
/// will cause cross-pollution of benchmarks so that's a no-go. The second
/// option will fill up the benchmarker's disk. Filling up disk is a no-go but
/// is something we can manage.
///
/// This struct keeps track of the current iteration of the tests and manages
/// new sub-data_dir paths. These paths, wrapped in a [`PathGuard`]
/// self-destruct when the benchmark drops them while this struct self-destructs
/// when it is dropped. This keeps disk consumption to a minimum.
struct DataDir {
    index: usize,
    base: PathBuf,
}

struct PathGuard {
    inner: PathBuf,
}

impl DataDir {
    fn new(name: &str) -> Self {
        let mut base_dir = PathBuf::new();
        base_dir.push(std::env::temp_dir());
        base_dir.push(name);
        std::fs::create_dir_all(&base_dir).expect("could not make base dir");

        Self {
            index: 0,
            base: base_dir,
        }
    }

    fn next(&mut self) -> PathGuard {
        let mut nxt = self.base.clone();
        nxt.push(&self.index.to_string());
        self.index += 1;
        std::fs::create_dir_all(&nxt).expect("could not make next dir");

        PathGuard { inner: nxt }
    }
}

impl Drop for DataDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.base).expect("could not remove base dir");
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.inner).expect("could not remove inner dir");
    }
}

fn create_disk_v1_variant(_max_events: usize, max_size: u64) -> BufferType {
    BufferType::DiskV1 {
        max_size,
        when_full: WhenFull::DropNewest,
    }
}

fn create_disk_v2_variant(_max_events: usize, max_size: u64) -> BufferType {
    BufferType::DiskV2 {
        max_size,
        when_full: WhenFull::DropNewest,
    }
}

fn create_in_memory_v1_variant(max_events: usize, _max_size: u64) -> BufferType {
    BufferType::MemoryV1 {
        max_events,
        when_full: WhenFull::DropNewest,
    }
}

fn create_in_memory_v2_variant(max_events: usize, _max_size: u64) -> BufferType {
    BufferType::MemoryV2 {
        max_events,
        when_full: WhenFull::DropNewest,
    }
}

macro_rules! experiment {
    ($criterion:expr, [$( $width:expr ),*], $group_name:expr, $id_slug:expr, $measure_fn:ident, $variant_fn:ident) => {{
        let mut group: BenchmarkGroup<WallTime> = $criterion.benchmark_group($group_name);
        group.sampling_mode(SamplingMode::Auto);
        init_instrumentation();

        let max_events: usize = 1_000;
        let mut data_dir = DataDir::new($id_slug);
        let rt = Runtime::new().unwrap();

        $(
            // Additional constant factor here is to avoid potential message
            // drops due to reuse of disk buffer's internals between
            // runs. Tempdir has low entropy compared to the number of
            // iterations we make in these benchmarks.
            let max_size = 1_000_000 * max_events as u64 * mem::size_of::<crate::common::Message<$width>>() as u64;
            let bytes = mem::size_of::<crate::common::Message<$width>>();
            group.throughput(Throughput::Elements(max_events as u64));
            group.bench_with_input(
                BenchmarkId::new($id_slug, bytes),
                &max_events,
                |b, max_events| {
                    b.to_async(&rt)
                        .iter_batched(
                            || {
                                let guard = data_dir.next();
                                let data_dir = guard.inner.clone();
                                let id = format!("{}-{}-{}", $group_name, $id_slug, $width);
                                let variant = $variant_fn(*max_events, max_size);

                                let (sender, receiver, messages) = tokio::task::block_in_place(move || {
                                    Handle::current().block_on(async move {
                                        crate::common::setup::<$width>(variant, *max_events, Some(data_dir), id).await
                                    })
                                });
                                (sender, receiver, messages, guard)
                            },
                            |(sender, receiver, messages, guard)| async move {
                                $measure_fn(sender, receiver, messages).await;
                                drop(guard)
                            },
                            BatchSize::SmallInput,
                        )
                },
            );
        )*
    }};
}

/// Writes all messages into the buffer, and then reads them all out.
fn write_then_read(c: &mut Criterion) {
    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-disk-v1",
        "write-then-read",
        wtr_measurement,
        create_disk_v1_variant
    );

    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-disk-v2",
        "write-then-read",
        wtr_measurement,
        create_disk_v2_variant
    );

    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-in-memory-v1",
        "write-then-read",
        wtr_measurement,
        create_in_memory_v1_variant
    );

    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-in-memory-v2",
        "write-then-read",
        wtr_measurement,
        create_in_memory_v2_variant
    );
}

/// Writes a message, and then reads a message, until all messages are gone.
fn write_and_read(c: &mut Criterion) {
    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-disk-v1",
        "write-and-read",
        war_measurement,
        create_disk_v1_variant
    );

    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-disk-v2",
        "write-and-read",
        war_measurement,
        create_disk_v2_variant
    );

    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-in-memory-v1",
        "write-and-read",
        war_measurement,
        create_in_memory_v1_variant
    );

    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-in-memory-v2",
        "write-and-read",
        war_measurement,
        create_in_memory_v2_variant
    );
}

criterion_group!(
    name = sized_records;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(60))
        .confidence_level(0.99)
        .nresamples(500_000)
        .sample_size(100);
    targets = write_then_read, write_and_read
);
criterion_main!(sized_records);
