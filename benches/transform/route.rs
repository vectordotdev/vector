use core::fmt;
use std::time::Duration;

use bytes::Bytes;
use criterion::{
    black_box, criterion_group, measurement::WallTime, BatchSize, BenchmarkGroup, BenchmarkId,
    Criterion, SamplingMode, Throughput,
};
use vector::config::TransformContext;
use vector::transforms::{
    route::{Route, RouteConfig},
    TransformOutputsBuf,
};
use vector_lib::{
    config::{DataType, TransformOutput},
    event::{Event, EventContainer, EventMetadata, LogEvent},
    transform::SyncTransform,
};
use vrl::value::{ObjectMap, Value};

#[derive(Debug)]
struct Param {
    slug: &'static str,
    input: Event,
    route_config: RouteConfig,
    output_buffer: TransformOutputsBuf,
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.slug)
    }
}

fn route(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> = c.benchmark_group("vector::transforms::route::Route");
    group.sampling_mode(SamplingMode::Auto);

    let mut fields = ObjectMap::new();
    for alpha in [
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r",
        "s", "t", "u", "v", "w", "x", "y", "z",
    ] {
        fields.insert(alpha.into(), Value::Bytes(Bytes::from(alpha)));
    }
    let event = Event::from(LogEvent::from_map(fields, EventMetadata::default()));

    let mut outputs = Vec::new();
    for name in [
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r",
        "s", "t", "u", "v", "w", "x", "y", "z", "aa", "ba", "ca", "da", "ea", "fa", "ga", "ha",
        "ia", "ja", "ka", "al", "ma", "na", "oa", "pa", "qa", "ra", "sa", "ta", "ua", "va", "wa",
        "xa", "ay", "za", "ba", "bb", "bc", "bd", "be", "bf", "bg", "bh", "bi", "bj", "bk", "bl",
        "bm", "bn", "bo", "bp", "bq", "br", "sb", "tb", "ub", "vb", "wb", "xb", "yb", "zb", "aba",
        "bba", "bbca", "dba", "bea", "fba", "gba", "hba", "iba", "jba", "bka", "bal", "bma", "bna",
        "boa", "bpa", "bqa", "bra", "bsa", "bta", "bua", "bva", "bwa", "xba", "aby", "zba",
    ] {
        outputs.push(TransformOutput {
            port: Some(String::from(name)),
            ty: DataType::Log,
            log_schema_definitions: Default::default(),
        });
    }
    let output_buffer: TransformOutputsBuf = TransformOutputsBuf::new_with_capacity(outputs, 10);

    for param in &[
        // A small filter where a sole field is mapped into a named route,
        // matches.
        Param {
            slug: "vrl_field_match",
            input: event.clone(),
            route_config: toml::from_str::<RouteConfig>(
                r#"
            route.a.type = "vrl"
            route.a.source = '.a == "aaa"'
        "#,
            )
            .unwrap(),
            output_buffer: output_buffer.clone(),
        },
        // A small filter where a sole field is mapped into a named route, does
        // not match.
        Param {
            slug: "vrl_field_not_match",
            input: event.clone(),
            route_config: toml::from_str::<RouteConfig>(
                r#"
            route.a.type = "vrl"
            route.a.source = '.a == "aaaaaaa"'
        "#,
            )
            .unwrap(),
            output_buffer: output_buffer.clone(),
        },
        // A larger filter where many fields are mapped, some multiple times,
        // into named filters. A mixture of match and not match happens.
        Param {
            slug: "vrl_field_match_many",
            input: event.clone(),
            route_config: toml::from_str::<RouteConfig>(
                r#"
            route.a.type = "vrl"
            route.a.source = '.a == "aaaaaaa"'

            route.b.type = "vrl"
            route.b.source = '.b == "b"'

            route.c.type = "vrl"
            route.c.source = '.c == "cccc"'

            route.d.type = "vrl"
            route.d.source = '.d == "d"'

            route.e.type = "vrl"
            route.e.source = '.d == "d"'

            route.f.type = "vrl"
            route.f.source = '.e == "eeeeeeeeee"'

            route.g.type = "vrl"
            route.g.source = '.f == "f"'

            route.h.type = "vrl"
            route.h.source = '.f == "qq"'

            route.i.type = "vrl"
            route.i.source = '.f == "fx"'

            route.j.type = "vrl"
            route.j.source = '.f == "lf"'

            route.k.type = "vrl"
            route.k.source = '.f == "fpioalkjasdf"'

            route.l.type = "vrl"
            route.l.source = '.f == "lkjiouasodifjlkjasdfoiuf"'

            route.m.type = "vrl"
            route.m.source = '.f == "aaaaf"'

            route.n.type = "vrl"
            route.n.source = '.f == "0124"'

            route.ay.type = "vrl"
            route.ay.source = '.a == "0_0"'
        "#,
            )
            .unwrap(),
            output_buffer: output_buffer.clone(),
        },
    ] {
        group.throughput(Throughput::Elements(param.input.len() as u64));
        group.bench_with_input(BenchmarkId::new("transform", param), &param, |b, param| {
            b.iter_batched(
                || {
                    let route =
                        Route::new(&param.route_config.clone(), &TransformContext::default())
                            .unwrap();
                    (route, param.input.clone(), param.output_buffer.clone())
                },
                |(mut route, input, mut output_buffer)| {
                    black_box(route.transform(input, &mut output_buffer));
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
    targets = route
);
