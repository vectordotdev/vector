use std::time::Duration;

use bytes::Bytes;
use criterion::{
    black_box, criterion_group, measurement::WallTime, BatchSize, BenchmarkGroup, Criterion,
    SamplingMode,
};
use datadog_grok::filters::keyvalue::{apply_filter, KeyValueFilter};
use regex::Regex;
use value::Value;

fn apply_filter_bench(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> =
        c.benchmark_group("datadog_grok::filters::keyvalue::apply_filter");
    group.sampling_mode(SamplingMode::Auto);

    group.bench_function("apply_filter key=valueStr", move |b| {
        b.iter_batched(
            || {
                let value = Value::Bytes(Bytes::from("key=valueStr"));
                let filter = KeyValueFilter {
                    key_value_delimiter: "=".into(),
                    value_re: Regex::new(r"^[\w.\-_@]+").unwrap(),
                    quotes: vec![('"', '"'), ('\'', '\''), ('<', '>')],
                    field_delimiters: [" ", ",", ";"]
                        .iter()
                        .map(|x| String::from(*x))
                        .collect::<Vec<String>>(),
                };
                (value, filter)
            },
            |(value, filter): (Value, KeyValueFilter)| {
                let _ = black_box(apply_filter(&value, &filter));
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("apply_filter key1=value1|key2=value2", move |b| {
        b.iter_batched(
            || {
                let value = Value::Bytes(Bytes::from("key1=value1|key2=value2"));
                let filter = KeyValueFilter {
                    key_value_delimiter: "=".into(),
                    value_re: Regex::new(r"^[\w.\-_@]+").unwrap(),
                    quotes: vec![('"', '"'), ('\'', '\''), ('<', '>')],
                    field_delimiters: ["|"]
                        .iter()
                        .map(|x| String::from(*x))
                        .collect::<Vec<String>>(),
                };
                (value, filter)
            },
            |(value, filter): (Value, KeyValueFilter)| {
                let _ = black_box(apply_filter(&value, &filter));
            },
            BatchSize::SmallInput,
        )
    });
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
    targets = apply_filter_bench
);
