use crate::common::{war_measurement, wtr_measurement};
use buffers::{self, Variant, WhenFull};
use criterion::{
    criterion_group, criterion_main, measurement::WallTime, BatchSize, BenchmarkGroup, BenchmarkId,
    Criterion, SamplingMode, Throughput,
};
use metrics_util::DebuggingRecorder;
use std::mem;
use std::path::PathBuf;
use std::time::Duration;

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

macro_rules! experiment {
    ($criterion:expr, [$( $width:expr ),*], $group_name:expr, $id_slug:expr, $measure_fn:ident) => {
        let mut group: BenchmarkGroup<WallTime> = $criterion.benchmark_group($group_name);
        group.sampling_mode(SamplingMode::Auto);
        if metrics::try_recorder().is_none() {
            DebuggingRecorder::new().install().unwrap();
        }

        let max_events = 1_000;
        let mut data_dir = DataDir::new($id_slug);

        $(
            // Additional constant factor here is to avoid potential message
            // drops due to reuse of disk buffer's internals between
            // runs. Tempdir has low entropy compared to the number of
            // iterations we make in these benchmarks.
            let max_size = 1_000_000 * max_events * mem::size_of::<crate::common::Message<$width>>();
            let bytes = mem::size_of::<crate::common::Message<$width>>();
            group.throughput(Throughput::Elements(max_events as u64));
            group.bench_with_input(
                BenchmarkId::new($id_slug, bytes),
                &max_events,
                |b, max_events| {
                    b.iter_batched(
                        || {
                            let guard = data_dir.next();
                            let variant = Variant::Disk {
                                max_size,
                                when_full: WhenFull::DropNewest,
                                data_dir: guard.inner.clone(),
                                id: format!("{}", $width),
                            };
                            let buf = crate::common::setup::<$width>(*max_events, variant);
                            (buf, guard)
                        },
                        |(buf, guard)| {
                            $measure_fn(buf);
                            drop(guard)
                        },
                        BatchSize::SmallInput,
                    )
                },
            );
        )*
    };
}

//
// [DISK] Write Then Read benchmark
//
// This benchmark uses the on-disk buffer with a sender/receiver that fully
// write all messages into the buffer, then fully read all messages. DropNewest
// is in effect when full condition is hit but sizes are carefully chosen to
// never fill the buffer.
//

fn write_then_read_disk(c: &mut Criterion) {
    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-disk",
        "write-then-read",
        wtr_measurement
    );
}

//
// [DISK] Write And Read benchmark
//
// This benchmark uses the on-disk buffer with a sender/receiver that write and
// read in lockstep. DropNewest is in effect when full condition is hit but
// sizes are carefully chosen to never fill the buffer.
//

fn write_and_read_disk(c: &mut Criterion) {
    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-disk",
        "write-and-read",
        war_measurement
    );
}

criterion_group!(
    name = on_disk;
    config = Criterion::default().measurement_time(Duration::from_secs(240)).confidence_level(0.99).nresamples(500_000).sample_size(100);
    targets = write_then_read_disk, write_and_read_disk
);
criterion_main!(on_disk);
