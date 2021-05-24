use buffers::{self, Variant, WhenFull};
use criterion::{
    criterion_group, criterion_main, measurement::WallTime, BatchSize, BenchmarkGroup, BenchmarkId,
    Criterion, SamplingMode, Throughput,
};
use std::mem;
use std::time::Duration;

mod common;

//
// [MEMORY] Write Then Read benchmark
//
// This benchmark uses the in-memory buffer with a sender/receiver that fully
// write all messages into the buffer, then fully read all messages. DropNewest
// is in effect when full condition is hit but sizes are carefully chosen to
// never fill the buffer.
//

macro_rules! write_then_read_memory {
    ($criterion:expr, [$( $width:expr ),*]) => {
        let mut group: BenchmarkGroup<WallTime> = $criterion.benchmark_group("buffer");
        group.sampling_mode(SamplingMode::Auto);

        let max_events = 1_000;
        $(
            let bytes = mem::size_of::<crate::common::Message<$width>>();
            group.throughput(Throughput::Elements(max_events as u64));
            group.bench_with_input(
                BenchmarkId::new("memory/write-then-read", bytes),
                &max_events,
                |b, max_events| {
                    b.iter_batched(
                        || {
                            let variant = Variant::Memory {
                                max_events: *max_events,
                                when_full: WhenFull::DropNewest,
                            };
                            crate::common::setup::<$width>(*max_events, variant)
                        },
                        crate::common::wtr_measurement,
                        BatchSize::SmallInput,
                    )
                },
            );
        )*
    };
}

fn write_then_read_memory(c: &mut Criterion) {
    write_then_read_memory!(c, [32, 64, 128, 256, 512, 1024]);
}

//
// [MEMORY] Write And Read benchmark
//
// This benchmark uses the in-memory buffer with a sender/receiver that write
// and read in lockstep. DropNewest is in effect when full condition is hit but
// sizes are carefully chosen to never fill the buffer.
//

macro_rules! write_and_read_memory {
    ($criterion:expr, [$( $width:expr ),*]) => {
        let mut group: BenchmarkGroup<WallTime> = $criterion.benchmark_group("buffer");
        group.sampling_mode(SamplingMode::Auto);

        let max_events = 1_000;
        $(
            let bytes = mem::size_of::<crate::common::Message<$width>>();
            group.throughput(Throughput::Elements(max_events as u64));
            group.bench_with_input(
                BenchmarkId::new("memory/write-and-read", bytes),
                &max_events,
                |b, max_events| {
                    b.iter_batched(
                        || {
                            let variant = Variant::Memory {
                                max_events: *max_events,
                                when_full: WhenFull::DropNewest,
                            };
                            crate::common::setup::<$width>(*max_events, variant)
                        },
                        crate::common::war_measurement,
                        BatchSize::SmallInput,
                    )
                },
            );
        )*
    };
}

fn write_and_read_memory(c: &mut Criterion) {
    write_and_read_memory!(c, [32, 64, 128, 256, 512, 1024]);
}

criterion_group!(
    name = in_memory;
    config = Criterion::default().measurement_time(Duration::from_secs(60)).confidence_level(0.99).nresamples(500_000).sample_size(250);
    targets = write_and_read_memory, write_then_read_memory
);
criterion_main!(in_memory);
