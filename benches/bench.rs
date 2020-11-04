use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};

use approx::assert_relative_eq;
use chrono::{DateTime, Utc};
use futures::{compat::Future01CompatExt, future, stream, StreamExt};
use indexmap::IndexMap;
use rand::{rngs::SmallRng, thread_rng, Rng, SeedableRng};
use rand_distr::{Alphanumeric, Distribution, Uniform};
use std::convert::TryFrom;

use vector::transforms::{
    add_fields::AddFields,
    coercer::CoercerConfig,
    json_parser::{JsonParser, JsonParserConfig},
    remap::{Remap, RemapConfig},
    Transform,
};
use vector::{
    config::{self, log_schema, TransformConfig},
    event::{Event, Value},
    sinks, sources,
    test_util::{
        next_addr, random_lines, runtime, send_lines, start_topology, wait_for_tcp, CountReceiver,
    },
    transforms,
};

mod batch;
mod buffering;
mod event;
mod files;
mod http;
mod lua;

criterion_group!(
    benches,
    benchmark_simple_pipes,
    benchmark_interconnected,
    benchmark_transforms,
    benchmark_complex,
    bench_elasticsearch_index,
    benchmark_regex,
    benchmark_remap,
);
criterion_main!(
    benches,
    buffering::buffers,
    http::http,
    batch::batch,
    files::files,
    lua::lua,
    event::event,
);

fn benchmark_simple_pipes(c: &mut Criterion) {
    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut group = c.benchmark_group("pipe");

    let benchmarks = [
        ("simple", 10_000, 100, 1),
        ("small_lines", 10_000, 1, 1),
        ("big_lines", 2_000, 10_000, 1),
        ("multiple_writers", 1_000, 100, 10),
    ];

    for (name, num_lines, line_size, num_writers) in benchmarks.iter() {
        group.throughput(Throughput::Bytes((num_lines * line_size) as u64));
        group.bench_function(format!("pipe_{}", name), |b| {
            b.iter_batched(
                || {
                    let mut config = config::Config::builder();
                    config.add_source(
                        "in",
                        sources::socket::SocketConfig::make_tcp_config(in_addr),
                    );
                    config.add_sink(
                        "out",
                        &["in"],
                        sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                            out_addr.to_string(),
                        ),
                    );

                    let mut rt = runtime();
                    let (output_lines, topology) = rt.block_on(async move {
                        let output_lines = CountReceiver::receive_lines(out_addr);
                        let (topology, _crash) =
                            start_topology(config.build().unwrap(), false).await;
                        wait_for_tcp(in_addr).await;
                        (output_lines, topology)
                    });
                    (rt, topology, output_lines)
                },
                |(mut rt, topology, output_lines)| {
                    rt.block_on(async move {
                        let sends = stream::iter(0..*num_writers)
                            .map(|_| {
                                let lines = random_lines(*line_size).take(*num_lines);
                                send_lines(in_addr, lines)
                            })
                            .collect::<Vec<_>>()
                            .await;
                        future::try_join_all(sends).await.unwrap();

                        topology.stop().compat().await.unwrap();

                        let output_lines = output_lines.await;

                        debug_assert_eq!(*num_lines * num_writers, output_lines.len());

                        output_lines
                    });
                },
                BatchSize::PerIteration,
            );
        });
    }

    group.finish();
}

fn benchmark_interconnected(c: &mut Criterion) {
    let num_lines: usize = 10_000;
    let line_size: usize = 100;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr1 = next_addr();
    let out_addr2 = next_addr();

    let mut group = c.benchmark_group("interconnected");
    group.throughput(Throughput::Bytes((num_lines * line_size * 2) as u64));

    group.bench_function("interconnected", |b| {
        b.iter_batched(
            || {
                let mut config = config::Config::builder();
                config.add_source(
                    "in1",
                    sources::socket::SocketConfig::make_tcp_config(in_addr1),
                );
                config.add_source(
                    "in2",
                    sources::socket::SocketConfig::make_tcp_config(in_addr2),
                );
                config.add_sink(
                    "out1",
                    &["in1", "in2"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr1.to_string()),
                );
                config.add_sink(
                    "out2",
                    &["in1", "in2"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr2.to_string()),
                );

                let mut rt = runtime();
                let (output_lines1, output_lines2, topology) = rt.block_on(async move {
                    let output_lines1 = CountReceiver::receive_lines(out_addr1);
                    let output_lines2 = CountReceiver::receive_lines(out_addr2);
                    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
                    wait_for_tcp(in_addr1).await;
                    wait_for_tcp(in_addr2).await;
                    (output_lines1, output_lines2, topology)
                });
                (rt, topology, output_lines1, output_lines2)
            },
            |(mut rt, topology, output_lines1, output_lines2)| {
                rt.block_on(async move {
                    let lines1 = random_lines(line_size).take(num_lines);
                    send_lines(in_addr1, lines1).await.unwrap();
                    let lines2 = random_lines(line_size).take(num_lines);
                    send_lines(in_addr2, lines2).await.unwrap();

                    topology.stop().compat().await.unwrap();

                    let output_lines1 = output_lines1.await;
                    let output_lines2 = output_lines2.await;

                    debug_assert_eq!(num_lines * 2, output_lines1.len());
                    debug_assert_eq!(num_lines * 2, output_lines2.len());

                    (output_lines1, output_lines2)
                });
            },
            BatchSize::PerIteration,
        );
    });

    group.finish();
}

fn benchmark_transforms(c: &mut Criterion) {
    let num_lines: usize = 10_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut group = c.benchmark_group("transforms");
    group.throughput(Throughput::Bytes(
        (num_lines * (line_size + "status=404".len())) as u64,
    ));

    group.bench_function("transforms", |b| {
        b.iter_batched(
            || {
                let mut config = config::Config::builder();
                config.add_source(
                    "in",
                    sources::socket::SocketConfig::make_tcp_config(in_addr),
                );
                config.add_transform(
                    "parser",
                    &["in"],
                    transforms::regex_parser::RegexParserConfig {
                        patterns: vec![r"status=(?P<status>\d+)".to_string()],
                        field: None,
                        ..Default::default()
                    },
                );
                config.add_transform(
                    "filter",
                    &["parser"],
                    transforms::field_filter::FieldFilterConfig {
                        field: "status".to_string(),
                        value: "404".to_string(),
                    },
                );
                config.add_sink(
                    "out",
                    &["filter"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
                );

                let mut rt = runtime();
                let (output_lines, topology) = rt.block_on(async move {
                    let output_lines = CountReceiver::receive_lines(out_addr);
                    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
                    wait_for_tcp(in_addr).await;
                    (output_lines, topology)
                });
                (rt, topology, output_lines)
            },
            |(mut rt, topology, output_lines)| {
                rt.block_on(async move {
                    let lines = random_lines(line_size)
                        .map(|l| l + "status=404")
                        .take(num_lines);
                    send_lines(in_addr, lines).await.unwrap();

                    topology.stop().compat().await.unwrap();

                    let output_lines = output_lines.await;

                    debug_assert_eq!(num_lines, output_lines.len());

                    output_lines
                });
            },
            BatchSize::PerIteration,
        );
    });

    group.finish();
}

fn benchmark_regex(c: &mut Criterion) {
    let lines: Vec<String> = http_access_log_lines().take(10).collect();

    let mut group = c.benchmark_group("regex");
    group.throughput(Throughput::Bytes(
        lines.iter().fold(0, |sum, l| sum + l.len()) as u64,
    ));

    let input: Vec<Event> = lines.into_iter().map(|l| l.into()).collect();

    group.bench_function("regex", |b| {
        let mut rt = runtime();

        let mut parser = rt.block_on(async move {
            transforms::regex_parser::RegexParserConfig {
                // Many captures to stress the regex parser
                patterns: vec![r#"^(?P<addr>\d+\.\d+\.\d+\.\d+) (?P<user>\S+) (?P<auth>\S+) \[(?P<date>\d+/[A-Za-z]+/\d+:\d+:\d+:\d+ [+-]\d{4})\] "(?P<method>[A-Z]+) (?P<uri>[^"]+) HTTP/\d\.\d" (?P<code>\d+) (?P<size>\d+) "(?P<referrer>[^"]+)" "(?P<browser>[^"]+)""#.into()],
                field: None,
                drop_failed: true,
                ..Default::default()
            }
            .build()
                .await
                .unwrap()
        });

        b.iter_batched(
            || {
                input.clone()
            },
            |events| {
                let event_count= events.len();
                let out_lines =events.into_iter()
                    .filter_map(|event| parser.transform(event))
                    .fold(0, |accum, _| accum + 1);

                debug_assert_eq!(out_lines,event_count);

                out_lines
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn benchmark_complex(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let sample_rate: u64 = 10;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr_all = next_addr();
    let out_addr_sampled = next_addr();
    let out_addr_200 = next_addr();
    let out_addr_404 = next_addr();
    let out_addr_500 = next_addr();

    c.bench_function("complex", |b| {
        b.iter_batched(
            || {
                let mut config = config::Config::builder();
                config.add_source(
                    "in1",
                    sources::socket::SocketConfig::make_tcp_config(in_addr1),
                );
                config.add_source(
                    "in2",
                    sources::socket::SocketConfig::make_tcp_config(in_addr2),
                );
                config.add_transform(
                    "parser",
                    &["in1", "in2"],
                    transforms::regex_parser::RegexParserConfig {
                        patterns: vec![r"status=(?P<status>\d+)".to_string()],
                        drop_field: false,
                        field: None,
                        ..Default::default()
                    },
                );
                config.add_transform(
                    "filter_200",
                    &["parser"],
                    transforms::field_filter::FieldFilterConfig {
                        field: "status".to_string(),
                        value: "200".to_string(),
                    },
                );
                config.add_transform(
                    "filter_404",
                    &["parser"],
                    transforms::field_filter::FieldFilterConfig {
                        field: "status".to_string(),
                        value: "404".to_string(),
                    },
                );
                config.add_transform(
                    "filter_500",
                    &["parser"],
                    transforms::field_filter::FieldFilterConfig {
                        field: "status".to_string(),
                        value: "500".to_string(),
                    },
                );
                config.add_transform(
                    "sampler",
                    &["parser"],
                    transforms::sampler::SamplerConfig {
                        rate: sample_rate,
                        key_field: None,
                        pass_list: vec![],
                    },
                );
                config.add_sink(
                    "out_all",
                    &["parser"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                        out_addr_all.to_string(),
                    ),
                );
                config.add_sink(
                    "out_sampled",
                    &["sampler"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                        out_addr_sampled.to_string(),
                    ),
                );
                config.add_sink(
                    "out_200",
                    &["filter_200"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                        out_addr_200.to_string(),
                    ),
                );
                config.add_sink(
                    "out_404",
                    &["filter_404"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                        out_addr_404.to_string(),
                    ),
                );
                config.add_sink(
                    "out_500",
                    &["filter_500"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                        out_addr_500.to_string(),
                    ),
                );

                let mut rt = runtime();
                let (
                    output_lines_all,
                    output_lines_sampled,
                    output_lines_200,
                    output_lines_404,
                    topology,
                ) = rt.block_on(async move {
                    let output_lines_all = CountReceiver::receive_lines(out_addr_all);
                    let output_lines_sampled = CountReceiver::receive_lines(out_addr_sampled);
                    let output_lines_200 = CountReceiver::receive_lines(out_addr_200);
                    let output_lines_404 = CountReceiver::receive_lines(out_addr_404);
                    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
                    wait_for_tcp(in_addr1).await;
                    wait_for_tcp(in_addr2).await;
                    (
                        output_lines_all,
                        output_lines_sampled,
                        output_lines_200,
                        output_lines_404,
                        topology,
                    )
                });
                (
                    rt,
                    topology,
                    output_lines_all,
                    output_lines_sampled,
                    output_lines_200,
                    output_lines_404,
                )
            },
            |(
                mut rt,
                topology,
                output_lines_all,
                output_lines_sampled,
                output_lines_200,
                output_lines_404,
            )| {
                rt.block_on(async move {
                    // One sender generates pure random lines
                    let lines1 = random_lines(100).take(num_lines);
                    send_lines(in_addr1, lines1).await.unwrap();

                    // The other includes either status=200 or status=404
                    let mut rng = SmallRng::from_rng(thread_rng()).unwrap();
                    let lines2 = random_lines(100)
                        .map(move |mut l| {
                            let status = if rng.gen_bool(0.5) { "200" } else { "404" };
                            l += "status=";
                            l += status;
                            l
                        })
                        .take(num_lines);
                    send_lines(in_addr2, lines2).await.unwrap();

                    topology.stop().compat().await.unwrap();

                    let output_lines_all = output_lines_all.await.len();
                    let output_lines_sampled = output_lines_sampled.await.len();
                    let output_lines_200 = output_lines_200.await.len();
                    let output_lines_404 = output_lines_404.await.len();

                    debug_assert_eq!(output_lines_all, num_lines * 2);
                    #[cfg(debug_assertions)]
                    {
                        // binomial distribution
                        let sample_stdev = (output_lines_all as f64
                            * (1f64 / sample_rate as f64)
                            * (1f64 - (1f64 / sample_rate as f64)))
                            .sqrt();

                        assert_relative_eq!(
                            output_lines_sampled as f64,
                            output_lines_all as f64 * (1f64 / sample_rate as f64),
                            epsilon = sample_stdev * 4f64 // should cover 99.993666% of cases
                        );
                    }
                    debug_assert!(output_lines_200 > 0);
                    debug_assert!(output_lines_404 > 0);
                    debug_assert_eq!(output_lines_200 + output_lines_404, num_lines);

                    (
                        output_lines_all,
                        output_lines_sampled,
                        output_lines_200,
                        output_lines_404,
                    )
                });
            },
            BatchSize::PerIteration,
        );
    });
}

fn bench_elasticsearch_index(c: &mut Criterion) {
    use vector::template::Template;

    let mut group = c.benchmark_group("elasticsearch_indexes");

    group.bench_function("dynamic", |b| {
        let index = Template::try_from("index-%Y.%m.%d").unwrap();
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key(), Utc::now());

        b.iter_batched(
            || event.clone(),
            |event| index.render(&event),
            BatchSize::SmallInput,
        )
    });

    group.bench_function("static", |b| {
        let index = Template::try_from("index").unwrap();
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key(), Utc::now());

        b.iter_batched(
            || event.clone(),
            |event| index.render(&event),
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn benchmark_remap(c: &mut Criterion) {
    let mut rt = runtime();
    let add_fields_runner = |tform: &mut Box<dyn Transform>, event: Event| {
        let result = tform.transform(event).unwrap();

        debug_assert_eq!(result.as_log().get("foo").unwrap().to_string_lossy(), "bar");
        debug_assert_eq!(result.as_log().get("bar").unwrap().to_string_lossy(), "baz");
        debug_assert_eq!(
            result.as_log().get("copy").unwrap().to_string_lossy(),
            "buz"
        );

        result
    };

    c.bench_function("remap: add fields with remap", |b| {
        let mut tform: Box<dyn Transform> = Box::new(
            Remap::new(RemapConfig {
                source: r#".foo = "bar"
            .bar = "baz"
            .copy = .copy_from"#
                    .to_string(),
                drop_on_err: true,
            })
            .unwrap(),
        );

        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("copy_from", "buz".to_owned());
            event
        };

        b.iter_batched(
            || event.clone(),
            |event| add_fields_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    c.bench_function("remap: add fields with add_fields", |b| {
        let mut fields = IndexMap::new();
        fields.insert("foo".into(), String::from("bar").into());
        fields.insert("bar".into(), String::from("baz").into());
        fields.insert("copy".into(), String::from("{{ copy_from }}").into());

        let mut tform: Box<dyn Transform> = Box::new(AddFields::new(fields, true).unwrap());

        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("copy_from", "buz".to_owned());
            event
        };

        b.iter_batched(
            || event.clone(),
            |event| add_fields_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    let json_parser_runner = |tform: &mut Box<dyn Transform>, event: Event| {
        let result = tform.transform(event).unwrap();

        debug_assert_eq!(
            result.as_log().get("foo").unwrap().to_string_lossy(),
            r#"{"key": "value"}"#
        );
        debug_assert_eq!(
            result.as_log().get("bar").unwrap().to_string_lossy(),
            r#"{"key":"value"}"#
        );

        result
    };

    c.bench_function("remap: parse JSON with remap", |b| {
        let mut tform: Box<dyn Transform> = Box::new(
            Remap::new(RemapConfig {
                source: ".bar = parse_json(.foo)".to_owned(),
                drop_on_err: false,
            })
            .unwrap(),
        );

        let event = {
            let mut event = Event::from("parse me");
            event
                .as_mut_log()
                .insert("foo", r#"{"key": "value"}"#.to_owned());
            event
        };

        b.iter_batched(
            || event.clone(),
            |event| json_parser_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    c.bench_function("remap: parse JSON with json_parser", |b| {
        let mut tform: Box<dyn Transform> = Box::new(JsonParser::from(JsonParserConfig {
            field: Some("foo".to_string()),
            target_field: Some("bar".to_owned()),
            drop_field: false,
            drop_invalid: false,
            overwrite_target: None,
        }));

        let event = {
            let mut event = Event::from("parse me");
            event
                .as_mut_log()
                .insert("foo", r#"{"key": "value"}"#.to_owned());
            event
        };

        b.iter_batched(
            || event.clone(),
            |event| json_parser_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    let coerce_runner = |tform: &mut Box<dyn Transform>, event: Event, timestamp: DateTime<Utc>| {
        let result = tform.transform(event).unwrap();

        debug_assert_eq!(
            result.as_log().get("number").unwrap(),
            &Value::Integer(1234)
        );
        debug_assert_eq!(result.as_log().get("bool").unwrap(), &Value::Boolean(true));
        debug_assert_eq!(
            result.as_log().get("timestamp").unwrap(),
            &Value::Timestamp(timestamp),
        );

        result
    };

    c.bench_function("remap: coerce with remap", |b| {
        let mut tform: Box<dyn Transform> = Box::new(
            Remap::new(RemapConfig {
                source: r#".number = to_int(.number)
                .bool = to_bool(.bool)
                .timestamp = parse_timestamp(.timestamp, format = "%d/%m/%Y:%H:%M:%S %z")
                "#
                .to_owned(),
                drop_on_err: true,
            })
            .unwrap(),
        );

        let mut event = Event::from("coerce me");
        for &(key, value) in &[
            ("number", "1234"),
            ("bool", "yes"),
            ("timestamp", "19/06/2019:17:20:49 -0400"),
        ] {
            event.as_mut_log().insert(key, value.to_owned());
        }

        let timestamp =
            DateTime::parse_from_str("19/06/2019:17:20:49 -0400", "%d/%m/%Y:%H:%M:%S %z")
                .unwrap()
                .with_timezone(&Utc);

        b.iter_batched(
            || event.clone(),
            |event| coerce_runner(&mut tform, event, timestamp),
            BatchSize::SmallInput,
        );
    });

    c.bench_function("remap: coerce with coercer", |b| {
        let mut tform: Box<dyn Transform> = rt.block_on(async move {
            toml::from_str::<CoercerConfig>(
                r#"drop_unspecified = false

                   [types]
                   number = "int"
                   bool = "bool"
                   timestamp = "timestamp|%d/%m/%Y:%H:%M:%S %z"
                   "#,
            )
            .unwrap()
            .build()
            .await
            .unwrap()
        });

        let mut event = Event::from("coerce me");
        for &(key, value) in &[
            ("number", "1234"),
            ("bool", "yes"),
            ("timestamp", "19/06/2019:17:20:49 -0400"),
        ] {
            event.as_mut_log().insert(key, value.to_owned());
        }

        let timestamp =
            DateTime::parse_from_str("19/06/2019:17:20:49 -0400", "%d/%m/%Y:%H:%M:%S %z")
                .unwrap()
                .with_timezone(&Utc);

        b.iter_batched(
            || event.clone(),
            |event| coerce_runner(&mut tform, event, timestamp),
            BatchSize::SmallInput,
        );
    });
}

fn http_access_log_lines() -> impl Iterator<Item = String> {
    let mut rng = SmallRng::from_rng(thread_rng()).unwrap();
    let code = Uniform::from(200..600);
    let year = Uniform::from(2010..2020);
    let mday = Uniform::from(1..32);
    let hour = Uniform::from(0..24);
    let minsec = Uniform::from(0..60);
    let size = Uniform::from(10..60); // FIXME

    std::iter::repeat(()).map(move |_| {
        let url_size = size.sample(&mut rng);
        let browser_size = size.sample(&mut rng);
        format!("{}.{}.{}.{} - - [{}/Jun/{}:{}:{}:{} -0400] \"GET /{} HTTP/1.1\" {} {} \"-\" \"Mozilla/5.0 ({})\"",
                rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(), // IP
                year.sample(&mut rng), mday.sample(&mut rng), // date
                hour.sample(&mut rng), minsec.sample(&mut rng), minsec.sample(&mut rng), // time
                (&mut rng).sample_iter(&Alphanumeric).take(url_size).collect::<String>(), // URL
                code.sample(&mut rng), size.sample(&mut rng),
                (&mut rng).sample_iter(&Alphanumeric).take(browser_size).collect::<String>(),
        )
    })
}
