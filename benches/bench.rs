use criterion::{criterion_group, criterion_main, Benchmark, Criterion, Throughput};

use approx::assert_relative_eq;
use futures::future;
use rand::distributions::{Alphanumeric, Uniform};
use rand::prelude::*;
use vector::event::Event;
use vector::test_util::{
    block_on, count_receive, next_addr, send_lines, shutdown_on_idle, wait_for_tcp,
};
use vector::topology::config::TransformConfig;
use vector::topology::{self, config};
use vector::{sinks, sources, transforms};

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
    event::event
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
                    let mut config = config::Config::empty();
                    config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
                    config.add_sink(
                        "out",
                        &["in"],
                        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
                    );

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_receive(&out_addr);

                    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
                    wait_for_tcp(in_addr);

                    (rt, topology, output_lines)
                },
                |(mut rt, topology, output_lines)| {
                    let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                    rt.block_on(send).unwrap();

                    block_on(topology.stop()).unwrap();

                    shutdown_on_idle(rt);
                    assert_eq!(num_lines, output_lines.wait());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size) as u32)),
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
                    let mut config = config::Config::empty();
                    config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
                    config.add_sink(
                        "out",
                        &["in"],
                        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
                    );

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_receive(&out_addr);

                    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
                    wait_for_tcp(in_addr);

                    (rt, topology, output_lines)
                },
                |(mut rt, topology, output_lines)| {
                    let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                    rt.block_on(send).unwrap();

                    block_on(topology.stop()).unwrap();

                    shutdown_on_idle(rt);
                    assert_eq!(num_lines, output_lines.wait());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size) as u32)),
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
                    let mut config = config::Config::empty();
                    config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
                    config.add_sink(
                        "out",
                        &["in"],
                        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
                    );

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_receive(&out_addr);

                    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
                    wait_for_tcp(in_addr);

                    (rt, topology, output_lines)
                },
                |(mut rt, topology, output_lines)| {
                    let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                    rt.block_on(send).unwrap();

                    block_on(topology.stop()).unwrap();

                    shutdown_on_idle(rt);
                    assert_eq!(num_lines, output_lines.wait());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size) as u32)),
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
                    let mut config = config::Config::empty();
                    config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
                    config.add_sink(
                        "out",
                        &["in"],
                        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
                    );

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_receive(&out_addr);

                    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
                    wait_for_tcp(in_addr);

                    (rt, topology, output_lines)
                },
                |(mut rt, topology, output_lines)| {
                    let sends = (0..num_writers)
                        .map(|_| {
                            let send = send_lines(in_addr, random_lines(line_size).take(num_lines));
                            futures::sync::oneshot::spawn(send, &rt.executor())
                        })
                        .collect::<Vec<_>>();

                    rt.block_on(future::join_all(sends)).unwrap();

                    std::thread::sleep(std::time::Duration::from_millis(100));

                    block_on(topology.stop()).unwrap();

                    shutdown_on_idle(rt);
                    assert_eq!(num_lines * num_writers, output_lines.wait());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes(
            (num_lines * line_size * num_writers) as u32,
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
                    let mut config = config::Config::empty();
                    config.add_source("in1", sources::tcp::TcpConfig::new(in_addr1));
                    config.add_source("in2", sources::tcp::TcpConfig::new(in_addr2));
                    config.add_sink(
                        "out1",
                        &["in1", "in2"],
                        sinks::tcp::TcpSinkConfig::new(out_addr1.to_string()),
                    );
                    config.add_sink(
                        "out2",
                        &["in1", "in2"],
                        sinks::tcp::TcpSinkConfig::new(out_addr2.to_string()),
                    );

                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines1 = count_receive(&out_addr1);
                    let output_lines2 = count_receive(&out_addr2);

                    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
                    wait_for_tcp(in_addr1);
                    wait_for_tcp(in_addr2);

                    (rt, topology, output_lines1, output_lines2)
                },
                |(mut rt, topology, output_lines1, output_lines2)| {
                    let send1 = send_lines(in_addr1, random_lines(line_size).take(num_lines));
                    let send2 = send_lines(in_addr2, random_lines(line_size).take(num_lines));
                    let sends = vec![send1, send2];
                    rt.block_on(future::join_all(sends)).unwrap();

                    block_on(topology.stop()).unwrap();

                    shutdown_on_idle(rt);
                    assert_eq!(num_lines * 2, output_lines1.wait());
                    assert_eq!(num_lines * 2, output_lines2.wait());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size * 2) as u32)),
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
                    let mut config = config::Config::empty();
                    config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
                    config.add_transform(
                        "parser",
                        &["in"],
                        transforms::regex_parser::RegexParserConfig {
                            regex: r"status=(?P<status>\d+)".to_string(),
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
                        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
                    );
                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines = count_receive(&out_addr);

                    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
                    wait_for_tcp(in_addr);

                    (rt, topology, output_lines)
                },
                |(mut rt, topology, output_lines)| {
                    let send = send_lines(
                        in_addr,
                        random_lines(line_size)
                            .map(|l| l + "status=404")
                            .take(num_lines),
                    );
                    rt.block_on(send).unwrap();

                    block_on(topology.stop()).unwrap();

                    shutdown_on_idle(rt);
                    assert_eq!(num_lines, output_lines.wait());
                },
            );
        })
        .sample_size(4)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes(
            (num_lines * (line_size + "status=404".len())) as u32,
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
                        regex: r#"^(?P<addr>\d+\.\d+\.\d+\.\d+) (?P<user>\S+) (?P<auth>\S+) \[(?P<date>\d+/[A-Za-z]+/\d+:\d+:\d+:\d+ [+-]\d{4})\] "(?P<method>[A-Z]+) (?P<uri>[^"]+) HTTP/\d\.\d" (?P<code>\d+) (?P<size>\d+) "(?P<referrer>[^"]+)" "(?P<browser>[^"]+)""#.into(),
                        field: None,
                        drop_failed: true,
                        ..Default::default()
                    }.build().unwrap();

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
                    let mut config = config::Config::empty();
                    config.add_source("in1", sources::tcp::TcpConfig::new(in_addr1));
                    config.add_source("in2", sources::tcp::TcpConfig::new(in_addr2));
                    config.add_transform(
                        "parser",
                        &["in1", "in2"],
                        transforms::regex_parser::RegexParserConfig {
                            regex: r"status=(?P<status>\d+)".to_string(),
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
                            pass_list: vec![],
                        },
                    );
                    config.add_sink(
                        "out_all",
                        &["parser"],
                        sinks::tcp::TcpSinkConfig::new(out_addr_all.to_string()),
                    );
                    config.add_sink(
                        "out_sampled",
                        &["sampler"],
                        sinks::tcp::TcpSinkConfig::new(out_addr_sampled.to_string()),
                    );
                    config.add_sink(
                        "out_200",
                        &["filter_200"],
                        sinks::tcp::TcpSinkConfig::new(out_addr_200.to_string()),
                    );
                    config.add_sink(
                        "out_404",
                        &["filter_404"],
                        sinks::tcp::TcpSinkConfig::new(out_addr_404.to_string()),
                    );
                    config.add_sink(
                        "out_500",
                        &["filter_500"],
                        sinks::tcp::TcpSinkConfig::new(out_addr_500.to_string()),
                    );
                    let mut rt = tokio::runtime::Runtime::new().unwrap();

                    let output_lines_all = count_receive(&out_addr_all);
                    let output_lines_sampled = count_receive(&out_addr_sampled);
                    let output_lines_200 = count_receive(&out_addr_200);
                    let output_lines_404 = count_receive(&out_addr_404);

                    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
                    wait_for_tcp(in_addr1);
                    wait_for_tcp(in_addr2);

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
                    // One sender generates pure random lines
                    let send1 = send_lines(in_addr1, random_lines(100).take(num_lines));
                    let send1 = futures::sync::oneshot::spawn(send1, &rt.executor());

                    // The other includes either status=200 or status=404
                    let mut rng = SmallRng::from_rng(thread_rng()).unwrap();
                    let send2 = send_lines(
                        in_addr2,
                        random_lines(100)
                            .map(move |mut l| {
                                let status = if rng.gen_bool(0.5) { "200" } else { "404" };
                                l += "status=";
                                l += status;
                                l
                            })
                            .take(num_lines),
                    );
                    let send2 = futures::sync::oneshot::spawn(send2, &rt.executor());
                    let sends = vec![send1, send2];
                    rt.block_on(future::join_all(sends)).unwrap();

                    block_on(topology.stop()).unwrap();

                    shutdown_on_idle(rt);

                    let output_lines_all = output_lines_all.wait();
                    let output_lines_sampled = output_lines_sampled.wait();
                    let output_lines_200 = output_lines_200.wait();
                    let output_lines_404 = output_lines_404.wait();

                    assert_eq!(output_lines_all, num_lines * 2);
                    assert_relative_eq!(
                        output_lines_sampled as f32 / num_lines as f32,
                        0.2,
                        epsilon = 0.01
                    );
                    assert!(output_lines_200 > 0);
                    assert!(output_lines_404 > 0);
                    assert_eq!(output_lines_200 + output_lines_404, num_lines);
                },
            );
        })
        .sample_size(2),
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
                        .insert_implicit(event::TIMESTAMP.clone(), Utc::now().into());

                    (Template::from("index-%Y.%m.%d"), event)
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
                        .insert_implicit(event::TIMESTAMP.clone(), Utc::now().into());

                    (Template::from("index"), event)
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
