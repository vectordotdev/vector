use core::fmt;
use std::{num::NonZeroUsize, time::Duration};

use criterion::{
    criterion_group, measurement::WallTime, BatchSize, BenchmarkGroup, BenchmarkId, Criterion,
    SamplingMode, Throughput,
};
use vector::transforms::dedupe::{CacheConfig, Dedupe, DedupeConfig, FieldMatchConfig};
use vector_core::transform::Transform;

use crate::common::{consume, FixedLogStream};

#[derive(Debug)]
struct Param {
    slug: &'static str,
    input: FixedLogStream,
    dedupe_config: DedupeConfig,
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.slug)
    }
}

fn dedupe(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector::transforms::dedupe::Dedupe");
    group.sampling_mode(SamplingMode::Auto);

    let fixed_stream = FixedLogStream::new(
        NonZeroUsize::new(128).unwrap(),
        NonZeroUsize::new(2).unwrap(),
    );
    for param in &[
        // Measurement where field "message" is ignored. This field is
        // automatically added by the LogEvent construction mechanism.
        Param {
            slug: "field_ignore_message",
            input: fixed_stream.clone(),
            dedupe_config: DedupeConfig {
                fields: Some(FieldMatchConfig::IgnoreFields(vec![String::from(
                    "message",
                )])),
                cache: CacheConfig { num_events: 4 },
            },
        },
        // Modification of previous where field "message" is matched.
        Param {
            slug: "field_match_message",
            input: fixed_stream.clone(),
            dedupe_config: DedupeConfig {
                fields: Some(FieldMatchConfig::MatchFields(vec![String::from("message")])),
                cache: CacheConfig { num_events: 4 },
            },
        },
        // Measurement where ignore fields do not exist in the event.
        Param {
            slug: "field_ignore_done",
            input: fixed_stream.clone(),
            dedupe_config: DedupeConfig {
                cache: CacheConfig { num_events: 4 },
                fields: Some(FieldMatchConfig::IgnoreFields(vec![
                    String::from("abcde"),
                    String::from("eabcd"),
                    String::from("deabc"),
                    String::from("cdeab"),
                    String::from("bcdea"),
                ])),
            },
        },
        // Modification of previous where match fields do not exist in the
        // event.
        Param {
            slug: "field_match_done",
            input: fixed_stream.clone(),
            dedupe_config: DedupeConfig {
                cache: CacheConfig { num_events: 4 },
                fields: Some(FieldMatchConfig::MatchFields(vec![
                    String::from("abcde"),
                    String::from("eabcd"),
                    String::from("deabc"),
                    String::from("cdeab"),
                    String::from("bcdea"),
                ])),
            },
        },
    ] {
        group.throughput(Throughput::Elements(param.input.len() as u64));
        group.bench_with_input(BenchmarkId::new("transform", param), &param, |b, param| {
            b.iter_batched(
                || {
                    let dedupe =
                        Transform::event_task(Dedupe::new(param.dedupe_config.clone())).into_task();
                    (Box::new(dedupe), Box::pin(param.input.clone()))
                },
                |(dedupe, input)| {
                    let output = dedupe.transform(input);
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
    targets = dedupe
);
