use buffers::{self, Variant, WhenFull};
use criterion::{
    criterion_group, criterion_main, measurement::WallTime, BatchSize, BenchmarkGroup, BenchmarkId,
    Criterion, SamplingMode, Throughput,
};
use std::mem;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
use std::{path::PathBuf, sync::atomic::Ordering};

mod common;

const ROTATION_THRESHOLD: usize = 10_000;

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
    counter: AtomicUsize,
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
            counter: AtomicUsize::new(0),
            base: base_dir,
        }
    }

    fn next(&mut self) -> PathGuard {
        let index = self.counter.fetch_add(1, Ordering::Relaxed);
        if index % ROTATION_THRESHOLD == 0 {
            // Because some filesystems have a limited number of directories
            // that are allowed under another we need to "rotate" as the
            // iterations proceed, that is, create a new sub-tree under the base
            // directory.
            let multiple = index / ROTATION_THRESHOLD;
            self.base.push(multiple.to_string());
        }
        let mut nxt = self.base.clone();
        nxt.push(&index.to_string());
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

//
// [DISK] Write Then Read benchmark
//
// This benchmark uses the on-disk buffer with a sender/receiver that fully
// write all messages into the buffer, then fully read all messages. DropNewest
// is in effect when full condition is hit but sizes are carefully chosen to
// never fill the buffer.
//

macro_rules! write_then_read_disk {
    ($criterion:expr, [$( $width:expr ),*]) => {
        let mut group: BenchmarkGroup<WallTime> = $criterion.benchmark_group("buffer-disk");
        group.sampling_mode(SamplingMode::Auto);

        let max_events = 1_000;
        let mut data_dir = DataDir::new("write-then-read");

        $(
            // Additional constant factor here is to avoid potential message
            // drops due to reuse of disk buffer's internals between
            // runs. Tempdir has low entropy compared to the number of
            // iterations we make in these benchmarks.
            let max_size = 1_000_000 * max_events * mem::size_of::<crate::common::Message<$width>>();
            let bytes = mem::size_of::<crate::common::Message<$width>>();
            group.throughput(Throughput::Elements(max_events as u64));
            group.bench_with_input(
                BenchmarkId::new("write-then-read", bytes),
                &max_events,
                |b, max_events| {
                    b.iter_batched(
                        || {
                            let guard = data_dir.next();
                            let variant = Variant::Disk {
                                max_size,
                                when_full: WhenFull::DropNewest,
                                data_dir: guard.inner.clone(),
                                name: format!("{}", $width),
                            };
                            let buf = crate::common::setup::<$width>(*max_events, variant);
                            (buf, guard)
                        },
                        |(buf, guard)| {
                            crate::common::wtr_measurement(buf);
                            drop(guard)
                        },
                        BatchSize::SmallInput,
                    )
                },
            );
        )*

    };
}

fn write_then_read_disk(c: &mut Criterion) {
    write_then_read_disk!(c, [32, 64, 128, 256, 512, 1024]);
}

//
// [DISK] Write And Read benchmark
//
// This benchmark uses the on-disk buffer with a sender/receiver that write and
// read in lockstep. DropNewest is in effect when full condition is hit but
// sizes are carefully chosen to never fill the buffer.
//

macro_rules! write_and_read_disk {
    ($criterion:expr, [$( $width:expr ),*]) => {
        let mut group: BenchmarkGroup<WallTime> = $criterion.benchmark_group("buffer-disk");
        group.sampling_mode(SamplingMode::Auto);

        let max_events = 1_000;
        let mut data_dir = DataDir::new("write-and-read");

        $(
            // Additional constant factor here is to avoid potential message
            // drops due to reuse of disk buffer's internals between
            // runs. Tempdir has low entropy compared to the number of
            // iterations we make in these benchmarks.
            let max_size = 1_000_000 * max_events * mem::size_of::<crate::common::Message<$width>>();
            let bytes = mem::size_of::<crate::common::Message<$width>>();
            group.throughput(Throughput::Elements(max_events as u64));
            group.bench_with_input(
                BenchmarkId::new("write-and-read", bytes),
                &max_events,
                |b, max_events| {
                    b.iter_batched(
                        || {
                            let guard = data_dir.next();
                            let variant = Variant::Disk {
                                max_size,
                                when_full: WhenFull::DropNewest,
                                data_dir: guard.inner.clone(),
                                name: format!("{}", $width),
                            };
                            let buf = crate::common::setup::<$width>(*max_events, variant);
                            (buf, guard)
                        },
                        |(buf, guard)| {
                            crate::common::war_measurement(buf);
                            drop(guard)
                        },
                        BatchSize::SmallInput,
                    )
                },
            );
        )*

    };
}

fn write_and_read_disk(c: &mut Criterion) {
    write_and_read_disk!(c, [32, 64, 128, 256, 512, 1024]);
}

criterion_group!(
    name = on_disk;
    config = Criterion::default().measurement_time(Duration::from_secs(60)).confidence_level(0.99).nresamples(500_000).sample_size(100);
    targets = write_then_read_disk, write_and_read_disk
);
criterion_main!(on_disk);
