use bytes::Bytes;
use criterion::{criterion_group, Benchmark, Criterion, Throughput};
use futures::{compat::Future01CompatExt, future};
use futures01::{Sink, Stream};
use std::{convert::Infallible, time::Duration};
use vector::{
    buffers::Acker,
    sinks::util::{
        batch::{Batch, BatchConfig, BatchError, BatchSettings, BatchSize, PushResult},
        BatchSink, Buffer, Compression, Partition, PartitionBatchSink,
    },
    test_util::{random_lines, runtime},
};

fn batching(
    bench_name: &'static str,
    compression: Compression,
    max_bytes: usize,
    num_events: usize,
    event_len: usize,
) -> Benchmark {
    Benchmark::new(bench_name, move |b| {
        b.iter_with_setup(
            move || {
                let input = random_lines(event_len)
                    .take(num_events)
                    .map(|s| s.into_bytes())
                    .collect::<Vec<_>>();
                futures01::stream::iter_ok::<_, ()>(input.into_iter())
            },
            |input| {
                let mut rt = runtime();
                let (acker, _) = Acker::new_for_testing();
                let batch = BatchSettings::default()
                    .bytes(max_bytes as u64)
                    .events(num_events)
                    .size;
                let batch_sink = BatchSink::new(
                    tower::service_fn(|_| future::ok::<_, Infallible>(())),
                    Buffer::new(batch, compression),
                    Duration::from_secs(1),
                    acker,
                )
                .sink_map_err(|e| panic!(e));

                let _ = rt.block_on(input.forward(batch_sink).compat()).unwrap();
            },
        )
    })
    .sample_size(10)
    .noise_threshold(0.05)
    .throughput(Throughput::Bytes((num_events * event_len) as u64))
}

fn partitioned_batching(
    bench_name: &'static str,
    compression: Compression,
    max_bytes: usize,
    num_events: usize,
    event_len: usize,
) -> Benchmark {
    Benchmark::new(bench_name, move |b| {
        b.iter_with_setup(
            move || {
                let key = Bytes::from("key");
                let input = random_lines(event_len)
                    .take(num_events)
                    .map(|s| s.into_bytes())
                    .map(|b| InnerBuffer {
                        inner: b,
                        key: key.clone(),
                    })
                    .collect::<Vec<_>>();
                futures01::stream::iter_ok::<_, ()>(input.into_iter())
            },
            |input| {
                let mut rt = runtime();
                let (acker, _) = Acker::new_for_testing();
                let batch = BatchSettings::default()
                    .bytes(max_bytes as u64)
                    .events(num_events)
                    .size;
                let batch_sink = PartitionBatchSink::new(
                    tower::service_fn(|_| future::ok::<_, Infallible>(())),
                    PartitionedBuffer::new(batch, compression),
                    Duration::from_secs(1),
                    acker,
                )
                .sink_map_err(|e| panic!(e));

                let _ = rt.block_on(input.forward(batch_sink).compat()).unwrap();
            },
        )
    })
    .sample_size(10)
    .noise_threshold(0.05)
    .throughput(Throughput::Bytes((num_events * event_len) as u64))
}

fn benchmark_batching(c: &mut Criterion) {
    c.bench(
        "batch",
        batching(
            "no compression 10mb with 2mb batches",
            Compression::None,
            2_000_000,
            100_000,
            100,
        ),
    );
    c.bench(
        "batch",
        batching(
            "gzip 10mb with 2mb batches",
            Compression::Gzip,
            2_000_000,
            100_000,
            100,
        ),
    );
    c.bench(
        "batch",
        batching(
            "gzip 10mb with 500kb batches",
            Compression::Gzip,
            500_000,
            100_000,
            100,
        ),
    );

    c.bench(
        "partitioned_batch",
        partitioned_batching(
            "no compression 10mb with 2mb batches",
            Compression::None,
            2_000_000,
            100_000,
            100,
        ),
    );
    c.bench(
        "partitioned_batch",
        partitioned_batching(
            "gzip 10mb with 2mb batches",
            Compression::Gzip,
            2_000_000,
            100_000,
            100,
        ),
    );
}

criterion_group!(batch, benchmark_batching);

pub struct PartitionedBuffer {
    inner: Buffer,
    key: Option<Bytes>,
}

#[derive(Clone)]
pub struct InnerBuffer {
    pub(self) inner: Vec<u8>,
    key: Bytes,
}

impl Partition<Bytes> for InnerBuffer {
    fn partition(&self) -> Bytes {
        self.key.clone()
    }
}

impl PartitionedBuffer {
    pub fn new(batch: BatchSize<Buffer>, compression: Compression) -> Self {
        Self {
            inner: Buffer::new(batch, compression),
            key: None,
        }
    }
}

impl Batch for PartitionedBuffer {
    type Input = InnerBuffer;
    type Output = InnerBuffer;

    fn get_settings_defaults(
        config: BatchConfig,
        defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError> {
        Ok(Buffer::get_settings_defaults(config, defaults.into())?.into())
    }

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        let key = item.key;
        match Batch::push(&mut self.inner, item.inner) {
            PushResult::Ok(full) => {
                self.key = Some(key);
                PushResult::Ok(full)
            }
            PushResult::Overflow(inner) => PushResult::Overflow(InnerBuffer { inner, key }),
        }
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn fresh(&self) -> Self {
        Self {
            inner: self.inner.fresh(),
            key: None,
        }
    }

    fn finish(mut self) -> Self::Output {
        let key = self.key.take().unwrap();
        let inner = self.inner.finish();
        InnerBuffer { inner, key }
    }

    fn num_items(&self) -> usize {
        self.inner.num_items()
    }
}
