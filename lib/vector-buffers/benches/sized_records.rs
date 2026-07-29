use std::{mem, time::Duration};

use criterion::{
    BatchSize, BenchmarkGroup, BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group,
    measurement::WallTime,
};
use tokio::runtime::{Handle, Runtime};

use crate::common::{
    BenchmarkState, DataDir, Message, Operation, disk_buffer, init_instrumentation,
    memory_buffer_by_bytes, memory_buffer_by_events,
};

macro_rules! experiment {
    ($criterion:expr, [$( $width:expr ),*], $group_name:expr, $operation:expr, $variant_fn:expr, $batch_size:expr) => {{
        let operation = $operation;
        let id_slug = operation.name();
        let mut group: BenchmarkGroup<WallTime> = $criterion.benchmark_group($group_name);
        group.sampling_mode(SamplingMode::Auto);
        init_instrumentation();

        let max_events: usize = 1_000;
        let mut data_dir = DataDir::new(id_slug);
        let rt = Runtime::new().unwrap();

        $(
            // Additional constant factor here is to avoid potential message
            // drops due to reuse of disk buffer's internals between
            // runs. Tempdir has low entropy compared to the number of
            // iterations we make in these benchmarks.
            let max_size = 1_000_000 * max_events as u64 * mem::size_of::<Message<$width>>() as u64;
            let bytes = mem::size_of::<Message<$width>>();
            group.throughput(Throughput::Elements(max_events as u64));
            group.bench_with_input(
                BenchmarkId::new(id_slug, bytes),
                &max_events,
                |b, max_events| {
                    b.to_async(&rt)
                        .iter_batched(
                            || {
                                let data_dir = data_dir.next();
                                let id = format!("{}-{}-{}", $group_name, id_slug, $width);
                                let variant = ($variant_fn)(*max_events, max_size);

                                tokio::task::block_in_place(move || {
                                    Handle::current().block_on(async move {
                                        BenchmarkState::<Message<$width>>::setup(
                                            variant,
                                            *max_events,
                                            Some(data_dir),
                                            id,
                                        )
                                        .await
                                    })
                                })
                            },
                            |state| operation.measure(state),
                            $batch_size,
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
        "buffer-disk",
        Operation::WriteThenRead,
        |_, max_size| disk_buffer(max_size),
        // Disk setup allocates a large backing file, so never batch multiple buffers.
        BatchSize::PerIteration
    );

    let f = |max_events, _| memory_buffer_by_events(max_events);
    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-in-memory",
        Operation::WriteThenRead,
        f,
        BatchSize::SmallInput
    );

    let f = |_, max_size| {
        memory_buffer_by_bytes(usize::try_from(max_size).expect("capacity must fit in usize"))
    };
    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-in-memory-bytes",
        Operation::WriteThenRead,
        f,
        BatchSize::SmallInput
    );
}

/// Writes a message, and then reads a message, until all messages are gone.
fn write_and_read(c: &mut Criterion) {
    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-disk",
        Operation::WriteAndRead,
        |_, max_size| disk_buffer(max_size),
        // Disk setup allocates a large backing file, so never batch multiple buffers.
        BatchSize::PerIteration
    );

    let f = |max_events, _| memory_buffer_by_events(max_events);
    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-in-memory",
        Operation::WriteAndRead,
        f,
        BatchSize::SmallInput
    );

    let f = |_, max_size| {
        memory_buffer_by_bytes(usize::try_from(max_size).expect("capacity must fit in usize"))
    };
    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-in-memory-bytes",
        Operation::WriteAndRead,
        f,
        BatchSize::SmallInput
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
