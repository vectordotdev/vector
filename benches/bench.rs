use criterion::{criterion_group, criterion_main, Benchmark, Criterion, Throughput};

use approx::assert_relative_eq;
use futures::{compat::Future01CompatExt, future, stream, StreamExt};
use rand::{
    distributions::{Alphanumeric, Uniform},
    prelude::*,
};
use std::convert::TryFrom;
use vector::{
    config::{self, TransformConfig, TransformContext},
    event::Event,
    sinks, sources,
    test_util::{next_addr, runtime, send_lines, start_topology, wait_for_tcp, CountReceiver},
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
    benchmark_simple_pipe,
    benchmark_simple_pipe_with_tiny_lines,
    benchmark_simple_pipe_with_huge_lines,
    benchmark_simple_pipe_with_many_writers,
    benchmark_interconnected,
    benchmark_transforms,
    benchmark_complex,
    bench_elasticsearch_index,
    benchmark_regex,
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

fn benchmark_simple_pipe(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    c.bench(
        "pipe",
        Benchmark::new("pipe", move |b| {
            b.iter_with_setup(
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
                        let lines = random_lines(line_size).take(num_lines);
                        send_lines(in_addr, lines).await.unwrap();

                        topology.stop().compat().await.unwrap();
                        assert_eq!(num_lines, output_lines.await.len());
                    });
                },
            );
        })
        .sample_size(10)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size) as u64)),
    );
}

fn benchmark_simple_pipe_with_tiny_lines(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 1;

    let in_addr = next_addr();
    let out_addr = next_addr();

    c.bench(
        "pipe_with_tiny_lines",
        Benchmark::new("pipe_with_tiny_lines", move |b| {
            b.iter_with_setup(
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
                        let lines = random_lines(line_size).take(num_lines);
                        send_lines(in_addr, lines).await.unwrap();

                        topology.stop().compat().await.unwrap();
                        assert_eq!(num_lines, output_lines.await.len());
                    });
                },
            );
        })
        .sample_size(10)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size) as u64)),
    );
}

fn benchmark_simple_pipe_with_huge_lines(c: &mut Criterion) {
    let num_lines: usize = 2_000;
    let line_size: usize = 100_000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    c.bench(
        "pipe_with_huge_lines",
        Benchmark::new("pipe_with_huge_lines", move |b| {
            b.iter_with_setup(
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
                        let lines = random_lines(line_size).take(num_lines);
                        send_lines(in_addr, lines).await.unwrap();

                        topology.stop().compat().await.unwrap();
                        assert_eq!(num_lines, output_lines.await.len());
                    });
                },
            );
        })
        .sample_size(10)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size) as u64)),
    );
}

fn benchmark_simple_pipe_with_many_writers(c: &mut Criterion) {
    let num_lines: usize = 10_000;
    let line_size: usize = 100;
    let num_writers: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    c.bench(
        "pipe_with_many_writers",
        Benchmark::new("pipe_with_many_writers", move |b| {
            b.iter_with_setup(
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
                        let sends = stream::iter(0..num_writers)
                            .map(|_| {
                                let lines = random_lines(line_size).take(num_lines);
                                send_lines(in_addr, lines)
                            })
                            .collect::<Vec<_>>()
                            .await;
                        future::try_join_all(sends).await.unwrap();

                        topology.stop().compat().await.unwrap();

                        // TODO: shutdown after flush
                        // assert_eq!(num_lines * num_writers, output_lines.await.len());
                        let _ = output_lines.await;
                    });
                },
            );
        })
        .sample_size(10)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes(
            (num_lines * line_size * num_writers) as u64,
        )),
    );
}

fn benchmark_interconnected(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 100;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr1 = next_addr();
    let out_addr2 = next_addr();

    c.bench(
        "interconnected",
        Benchmark::new("interconnected", move |b| {
            b.iter_with_setup(
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
                        sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                            out_addr1.to_string(),
                        ),
                    );
                    config.add_sink(
                        "out2",
                        &["in1", "in2"],
                        sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                            out_addr2.to_string(),
                        ),
                    );

                    let mut rt = runtime();
                    let (output_lines1, output_lines2, topology) = rt.block_on(async move {
                        let output_lines1 = CountReceiver::receive_lines(out_addr1);
                        let output_lines2 = CountReceiver::receive_lines(out_addr2);
                        let (topology, _crash) =
                            start_topology(config.build().unwrap(), false).await;
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
                        assert_eq!(num_lines * 2, output_lines1.await.len());
                        assert_eq!(num_lines * 2, output_lines2.await.len());
                    });
                },
            );
        })
        .sample_size(10)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size * 2) as u64)),
    );
}

fn benchmark_transforms(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    c.bench(
        "transforms",
        Benchmark::new("transforms", move |b| {
            b.iter_with_setup(
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
                        let lines = random_lines(line_size)
                            .map(|l| l + "status=404")
                            .take(num_lines);
                        send_lines(in_addr, lines).await.unwrap();

                        topology.stop().compat().await.unwrap();
                        assert_eq!(num_lines, output_lines.await.len());
                    });
                },
            );
        })
        .sample_size(10)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes(
            (num_lines * (line_size + "status=404".len())) as u64,
        )),
    );
}

fn benchmark_regex(c: &mut Criterion) {
    let num_lines: usize = 100_000;

    c.bench(
        "regex",
        Benchmark::new("regex", move |b| {
            b.iter_with_setup(
                || {
                    let parser =transforms::regex_parser::RegexParserConfig {
                        // Many captures to stress the regex parser
                        patterns: vec![r#"^(?P<addr>\d+\.\d+\.\d+\.\d+) (?P<user>\S+) (?P<auth>\S+) \[(?P<date>\d+/[A-Za-z]+/\d+:\d+:\d+:\d+ [+-]\d{4})\] "(?P<method>[A-Z]+) (?P<uri>[^"]+) HTTP/\d\.\d" (?P<code>\d+) (?P<size>\d+) "(?P<referrer>[^"]+)" "(?P<browser>[^"]+)""#.into()],
                        field: None,
                        drop_failed: true,
                        ..Default::default()
                    }.build(TransformContext::new_test()).unwrap();

                    let src_lines = http_access_log_lines()
                        .take(num_lines)
                        .collect::<Vec<String>>();

                    (parser, src_lines)
                },
                |(mut parser, src_lines)| {
                    let out_lines = src_lines.iter()
                        .filter_map(|line| parser.transform(Event::from(&line[..])))
                        .fold(0, |accum, _| accum + 1);

                    assert_eq!(out_lines, num_lines);
                },
            );
        })
        .sample_size(10)
        .noise_threshold(0.05)
    );
}

fn benchmark_complex(c: &mut Criterion) {
    let num_lines: usize = 100_000;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr_all = next_addr();
    let out_addr_sampled = next_addr();
    let out_addr_200 = next_addr();
    let out_addr_404 = next_addr();
    let out_addr_500 = next_addr();

    c.bench(
        "complex",
        Benchmark::new("complex", move |b| {
            b.iter_with_setup(
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
                            rate: 10,
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
                        let (topology, _crash) =
                            start_topology(config.build().unwrap(), false).await;
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

                        assert_eq!(output_lines_all, num_lines * 2);
                        assert_relative_eq!(
                            output_lines_sampled as f32 / num_lines as f32,
                            0.1,
                            epsilon = 0.01
                        );
                        assert!(output_lines_200 > 0);
                        assert!(output_lines_404 > 0);
                        assert_eq!(output_lines_200 + output_lines_404, num_lines);
                    });
                },
            );
        })
        .sample_size(10),
    );
}

fn bench_elasticsearch_index(c: &mut Criterion) {
    use chrono::Utc;
    use vector::{event, template::Template};

    c.bench(
        "elasticsearch_indexes",
        Benchmark::new("dynamic", move |b| {
            b.iter_with_setup(
                || {
                    let mut event = Event::from("hello world");
                    event
                        .as_mut_log()
                        .insert(event::log_schema().timestamp_key().clone(), Utc::now());

                    (Template::try_from("index-%Y.%m.%d").unwrap(), event)
                },
                |(index, event)| index.render(&event),
            )
        }),
    );

    c.bench(
        "elasticsearch_indexes",
        Benchmark::new("static", move |b| {
            b.iter_with_setup(
                || {
                    let mut event = Event::from("hello world");
                    event
                        .as_mut_log()
                        .insert(event::log_schema().timestamp_key().clone(), Utc::now());

                    (Template::try_from("index").unwrap(), event)
                },
                |(index, event)| index.render(&event),
            )
        }),
    );
}

fn random_lines(size: usize) -> impl Iterator<Item = String> {
    let mut rng = SmallRng::from_rng(thread_rng()).unwrap();

    std::iter::repeat(()).map(move |_| {
        rng.sample_iter(&Alphanumeric)
            .take(size)
            .collect::<String>()
    })
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
                rng.sample_iter(&Alphanumeric).take(url_size).collect::<String>(), // URL
                code.sample(&mut rng), size.sample(&mut rng),
                rng.sample_iter(&Alphanumeric).take(browser_size).collect::<String>(),
        )
    })
}
