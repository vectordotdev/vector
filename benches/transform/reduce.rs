use core::fmt;
use std::{num::NonZeroUsize, time::Duration};

use criterion::{
    criterion_group, measurement::WallTime, BatchSize, BenchmarkGroup, BenchmarkId, Criterion,
    SamplingMode, Throughput,
};
use indexmap::IndexMap;
use vector::transforms::reduce::{Reduce, ReduceConfig};
use vector_lib::transform::Transform;

use crate::common::{consume, FixedLogStream};

#[derive(Debug)]
struct Param {
    slug: &'static str,
    input: FixedLogStream,
    reduce_config: ReduceConfig,
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.slug,)
    }
}

fn reduce(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector::transforms::reduce::Reduce");
    group.sampling_mode(SamplingMode::Auto);

    let fixed_stream = FixedLogStream::new(
        NonZeroUsize::new(128).unwrap(),
        NonZeroUsize::new(2).unwrap(),
    );
    for param in &[
        // The `Reduce` transform has a high configuration surface. For now we
        // only benchmark the "proof of concept" configuration, demonstrating
        // that the benchmark does minimally work. Once we have soak tests with
        // reduces in them we should extend this array to include those
        // configurations.
        Param {
            slug: "proof_of_concept",
            input: fixed_stream.clone(),
            reduce_config: ReduceConfig {
                expire_after_ms: Duration::from_secs(30),
                flush_period_ms: Duration::from_secs(1),
                group_by: vec![String::from("message")],
                merge_strategies: IndexMap::default(),
                ends_when: None,
                starts_when: None,
                max_events: None,
            },
        },
    ] {
        group.throughput(Throughput::Elements(param.input.len() as u64));
        group.bench_with_input(BenchmarkId::new("transform", param), &param, |b, param| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter_batched(
                    || {
                        let reduce = Transform::event_task(
                            Reduce::new(&param.reduce_config, &Default::default()).unwrap(),
                        )
                        .into_task();
                        (Box::new(reduce), Box::pin(param.input.clone()))
                    },
                    |(reduce, input)| async {
                        let output = reduce.transform_events(input);
                        consume(output)
                    },
                    BatchSize::SmallInput,
                )
        });
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(5))
        .measurement_time(Duration::from_secs(120))
        // degree of noise to ignore in measurements, here 1%
        .noise_threshold(0.01)
        // likelihood of noise registering as difference, here 5%
        .significance_level(0.05)
        // likelihood of capturing the true runtime, here 95%
        .confidence_level(0.95)
        // total number of bootstrap resamples, higher is less noisy but slower
        .nresamples(100_000)
        // total samples to collect within the set measurement time
        .sample_size(150);
    targets = reduce
);
