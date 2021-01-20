use criterion::{criterion_group, criterion_main, BatchSize, Criterion, SamplingMode, Throughput};
use futures::{compat::Future01CompatExt, stream, StreamExt};
use vector::{
    config,
    test_util::{next_addr, runtime, send_lines, start_topology, wait_for_tcp, CountReceiver},
};

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_parse_syslog
);
criterion_main!(benches);

fn benchmark_parse_syslog(c: &mut Criterion) {
    vector::test_util::trace_init();

    let num_lines = 10_000;
    let in_addr = next_addr();
    let out_addr = next_addr();

    let configs: Vec<(&str, &str)> = vec![
        (
            "remap",
            r#"
[transforms.last]
  type = "remap"
  inputs = ["in"]
  source = """
      . = parse_syslog(.message)
  """
         "#,
        ),
        (
            "native",
            r#"
[transforms.last]
  type = "regex_parser"
  inputs = ["in"]
  field = "message"
  patterns = ['^<(?P<priority>\d+)>(?P<version>\d+) (?P<timestamp>%S+) (?P<hostname>%S+) (?P<appname>%S+) (?P<procid>\S+) (?P<msgid>\S+) (?P<sdata>%S+) (?P<message>.+)$']
  types.appname = "string"
  types.facility = "string"
  types.hostname = "string"
  types.level = "string"
  types.message = "string"
  types.msgid = "string"
  types.procid = "int"
  types.timestamp = "timestamp|%F"
         "#,
        ),
        (
            "lua",
            r#"
[transforms.last]
  type = "lua"
  inputs = ["in"]
  version = "2"
  source = """
  local function parse_syslog(message)
    local pattern = "^<(%d+)>(%d+) (%S+) (%S+) (%S+) (%S+) (%S+) (%S+) (.+)$"
    local priority, version, timestamp, host, appname, procid, msgid, sdata, message = string.match(message, pattern)

    return {priority = priority, version = version, timestamp = timestamp, host = host, appname = appname, procid = procid, msgid = msgid, sdata = sdata, message = message}
  end

  function process(event, emit)
    event.log = parse_syslog(event.log.message)
    emit(event)
  end
  """
  hooks.process = "process"
        "#,
        ),
        (
            "wasm",
            r#"
[transforms.last]
  type = "lua"
  inputs = ["in"]
  module = "tests/data/wasm/parse_syslog/target/wasm32-wasi/release/parse_syslog.wasm"
  artifact_cache = "target/artifacts/"
        "#,
        ),
    ];

    let mut group = c.benchmark_group("language/parse_syslog");
    group.sampling_mode(SamplingMode::Flat);

    let source_config = format!(
        r#"
[sources.in]
  type = "socket"
  mode = "tcp"
  address = "{}"
"#,
        in_addr
    );
    let sink_config = format!(
        r#"
[sinks.out]
  inputs = ["last"]
  type = "socket"
  mode = "tcp"
  encoding.codec = "json"
  address = "{}"
"#,
        out_addr
    );

    for (name, transform_config) in configs.into_iter() {
        group.throughput(Throughput::Elements(num_lines as u64));
        group.bench_function(name.clone(), |b| {
            b.iter_batched(
                || {
                    let mut config = source_config.clone();
                    config.push_str(&transform_config);
                    config.push_str(&sink_config);

                    let config =  config::load_from_str(&config, Some(config::Format::TOML)).unwrap();
                    let mut rt = runtime();
                    let (output_lines, topology) = rt.block_on(async move {
                        let output_lines = CountReceiver::receive_lines(out_addr);
                        let (topology, _crash) =
                            start_topology(config, false).await;
                        wait_for_tcp(in_addr).await;
                        (output_lines, topology)
                    });
                    let lines = rt.block_on(async move {
                        // TODO randomize lines
                        //stream::repeat(vector::sources::util::fake::syslog_5424_log_line())
                            //.take(num_lines)
                            //.collect::<Vec<_>>()
                            //.await
                        stream::repeat(r#"<12>3 2020-12-19T21:48:09.004Z initech.io su 4015 ID81 - TPS report missing cover sheet"#.to_string()).take(num_lines).collect::<Vec<_>>().await
                    });
                    (rt, lines, topology, output_lines)
                },
                |(mut rt, lines, topology, output_lines)| {
                    rt.block_on(async move {
                        send_lines(in_addr, lines).await.unwrap();

                        topology.stop().compat().await.unwrap();

                        let output_lines = output_lines.await;

                        #[cfg(debug_assertions)]
                        {
                            // TODO assert lines are tranformed
                            assert_eq!(num_lines, output_lines.len());
                        }

                        output_lines
                    });
                },
                BatchSize::PerIteration,
            );
        });
    }

    group.finish();
}
