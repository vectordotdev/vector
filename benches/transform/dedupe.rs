use core::fmt;
use std::{num::NonZeroUsize, time::Duration};

use crate::common::{consume, FixedLogStream};
use criterion::{
    criterion_group, measurement::WallTime, BatchSize, BenchmarkGroup, BenchmarkId, Criterion,
    SamplingMode, Throughput,
};
use vector::transforms::dedupe::common::{CacheConfig, FieldMatchConfig, TimedCacheConfig};
use vector::transforms::dedupe::config::DedupeConfig;
use vector::transforms::dedupe::transform::Dedupe;
use vector_lib::transform::Transform;

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
    let cache = CacheConfig {
        num_events: NonZeroUsize::new(4).unwrap(),
    };
    for param in &[
        // Measurement where field "message" is ignored. This field is
        // automatically added by the LogEvent construction mechanism.
        Param {
            slug: "field_ignore_message",
            input: fixed_stream.clone(),
            dedupe_config: DedupeConfig {
                fields: Some(FieldMatchConfig::IgnoreFields(vec!["message".into()])),
                cache: cache.clone(),
                time_settings: None,
            },
        },
        // Modification of previous where field "message" is matched.
        Param {
            slug: "field_match_message",
            input: fixed_stream.clone(),
            dedupe_config: DedupeConfig {
                fields: Some(FieldMatchConfig::MatchFields(vec!["message".into()])),
                cache: cache.clone(),
                time_settings: None,
            },
        },
        // Modification of previous where deduplication with max age is used.
        Param {
            slug: "field_match_message_timed",
            input: fixed_stream.clone(),
            dedupe_config: DedupeConfig {
                fields: Some(FieldMatchConfig::MatchFields(vec!["message".into()])),
                cache: cache.clone(),
                time_settings: Some(TimedCacheConfig {
                    max_age_ms: Duration::from_secs(5),
                    refresh_on_drop: false,
                }),
            },
        },
        // Modification of previous where refresh on drop is enabled.
        Param {
            slug: "field_match_message_timed_refresh_on_drop",
            input: fixed_stream.clone(),
            dedupe_config: DedupeConfig {
                fields: Some(FieldMatchConfig::MatchFields(vec!["message".into()])),
                cache: cache.clone(),
                time_settings: Some(TimedCacheConfig {
                    max_age_ms: Duration::from_secs(5),
                    refresh_on_drop: true,
                }),
            },
        },
        // Measurement where ignore fields do not exist in the event.
        Param {
            slug: "field_ignore_done",
            input: fixed_stream.clone(),
            dedupe_config: DedupeConfig {
                cache: cache.clone(),
                fields: Some(FieldMatchConfig::IgnoreFields(vec![
                    "abcde".into(),
                    "eabcd".into(),
                    "deabc".into(),
                    "cdeab".into(),
                    "bcdea".into(),
                ])),
                time_settings: None,
            },
        },
        // Modification of previous where match fields do not exist in the
        // event.
        Param {
            slug: "field_match_done",
            input: fixed_stream.clone(),
            dedupe_config: DedupeConfig {
                cache,
                fields: Some(FieldMatchConfig::MatchFields(vec![
                    "abcde".into(),
                    "eabcd".into(),
                    "deabc".into(),
                    "cdeab".into(),
                    "bcdea".into(),
                ])),
                time_settings: None,
            },
        },
    ] {
        group.throughput(Throughput::Elements(param.input.len() as u64));
        group.bench_with_input(BenchmarkId::new("transform", param), &param, |b, param| {
            b.iter_batched(
                || {
                    let config = param.dedupe_config.clone();
                    let dedupe = Transform::event_task(Dedupe::new(
                        config.cache.num_events,
                        config.fields.unwrap(),
                    ))
                    .into_task();
                    (Box::new(dedupe), Box::pin(param.input.clone()))
                },
                |(dedupe, input)| {
                    let output = dedupe.transform_events(input);
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
