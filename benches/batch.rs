use bytes::Bytes;
use criterion::{criterion_group, Criterion, SamplingMode, Throughput};
use futures::{future, stream, SinkExt, StreamExt};
use std::{convert::Infallible, time::Duration};
use vector::{
    buffers::Acker,
    sinks::util::{
        batch::{Batch, BatchConfig, BatchError, BatchSettings, BatchSize, PushResult},
        BatchSink, Buffer, Compression, Partition, PartitionBatchSink,
    },
    test_util::{random_lines, runtime},
};

fn benchmark_batching(c: &mut Criterion) {
    let event_len: usize = 100;
    let num_events: usize = 100_000;

    let mut group = c.benchmark_group("partitioned_batching");
    group.throughput(Throughput::Bytes((event_len * num_events) as u64));
    group.sampling_mode(SamplingMode::Flat);

    let cases = [
        (Compression::None, bytesize::mib(2u64)),
        (Compression::None, bytesize::kib(500u64)),
        (Compression::gzip_default(), bytesize::mib(2u64)),
        (Compression::gzip_default(), bytesize::kib(500u64)),
    ];

    let input: Vec<_> = random_lines(event_len)
        .take(num_events)
        .map(|s| s.into_bytes())
        .collect();

    for (compression, batch_size) in cases.iter() {
        group.bench_function(
            format!("partitioned_batching_{}_{}", compression, batch_size),
            |b| {
                b.iter_batched(
                    || {
                        let rt = runtime();
                        let (acker, _) = Acker::new_for_testing();
                        let batch = BatchSettings::default()
                            .bytes(*batch_size as u64)
                            .events(num_events)
                            .size;
                        let batch_sink = PartitionBatchSink::new(
                            tower::service_fn(|_| future::ok::<_, Infallible>(())),
                            PartitionedBuffer::new(batch, *compression),
                            Duration::from_secs(1),
                            acker,
                        )
                        .sink_map_err(|error| panic!(error));

                        (
                            rt,
                            stream::iter(input.clone().into_iter().map(|b| InnerBuffer {
                                inner: b,
                                key: Bytes::from("key"),
                            }))
                            .map(Ok),
                            batch_sink,
                        )
                    },
                    |(mut rt, input, batch_sink)| rt.block_on(input.forward(batch_sink)).unwrap(),
                    criterion::BatchSize::LargeInput,
                )
            },
        );

        group.bench_function(format!("batching_{}_{}", compression, batch_size), |b| {
            b.iter_batched(
                || {
                    let rt = runtime();
                    let (acker, _) = Acker::new_for_testing();
                    let batch = BatchSettings::default()
                        .bytes(*batch_size as u64)
                        .events(num_events)
                        .size;
                    let batch_sink = BatchSink::new(
                        tower::service_fn(|_| future::ok::<_, Infallible>(())),
                        Buffer::new(batch, *compression),
                        Duration::from_secs(1),
                        acker,
                    )
                    .sink_map_err(|error| panic!(error));

                    (rt, stream::iter(input.clone()).map(Ok), batch_sink)
                },
                |(mut rt, input, batch_sink)| rt.block_on(input.forward(batch_sink)).unwrap(),
                criterion::BatchSize::LargeInput,
            )
        });
    }
}

criterion_group!(
    name = benches;
    // noisy benchmarks; 10% encapsulates what we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.10);
    targets = benchmark_batching
);

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
