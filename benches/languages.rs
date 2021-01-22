use criterion::{criterion_group, criterion_main, BatchSize, Criterion, SamplingMode, Throughput};
use futures::compat::Future01CompatExt;
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
    let configs: Vec<(&str, &str)> = vec![
        (
            "remap",
            r#"
[transforms.last]
  type = "remap"
  inputs = ["in"]
  source = """
      . = parse_syslog!(.message)
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
  type = "wasm"
  inputs = ["in"]
  module = "tests/data/wasm/parse_syslog/target/wasm32-wasi/release/parse_syslog.wasm"
  artifact_cache = "target/artifacts/"
        "#,
        ),
    ];

    let input = r#"<12>3 2020-12-19T21:48:09.004Z initech.io su 4015 ID81 - TPS report missing cover sheet"#;
    let output = serde_json::from_str(r#"{ "appname": "su", "facility": "user", "hostname": "initech.io", "severity": "warning", "message": "TPS report missing cover sheet", "msgid": "ID81", "procid": 4015, "timestamp": "2020-12-19 21:48:09.004 +00:00", "version": 3 }"#).unwrap();

    benchmark_configs(c, "parse_syslog", configs, "in", "last", input, output);
}

/// Benches a set of transform configs for comparison
///
/// # Arguments
///
/// * `criterion` - Criterion benchmark manager
/// * `benchmark_name` - The name of the benchmark
/// * `configs' - Vec of tuples of (config_name, config_snippet)
/// * `input_name` - Name of the input to the first transform
/// * `output_name` - Name of the last transform
/// * `input` - Line to use as input
/// * `output` - Expected transformed line as JSON value
fn benchmark_configs(
    criterion: &mut Criterion,
    benchmark_name: &str,
    configs: Vec<(&str, &str)>,
    input_name: &str,
    output_name: &str,
    input: &str,
    output: serde_json::Value,
) {
    vector::test_util::trace_init();

    // only used for debug assertions so assigned to supress unused warning
    let _ = output;

    let num_lines = 10_000;
    let in_addr = next_addr();
    let out_addr = next_addr();

    let lines: Vec<_> = ::std::iter::repeat(input.to_string())
        .take(num_lines)
        .collect();

    let mut group = criterion.benchmark_group(format!("language/{}", benchmark_name));
    group.sampling_mode(SamplingMode::Flat);

    let source_config = format!(
        r#"
[sources.{}]
  type = "socket"
  mode = "tcp"
  address = "{}"
"#,
        input_name, in_addr
    );
    let sink_config = format!(
        r#"
[sinks.out]
  inputs = ["{}"]
  type = "socket"
  mode = "tcp"
  encoding.codec = "json"
  address = "{}"
"#,
        output_name, out_addr
    );

    for (name, transform_config) in configs.into_iter() {
        group.throughput(Throughput::Elements(num_lines as u64));
        group.bench_function(name.clone(), |b| {
            b.iter_batched(
                || {
                    let mut config = source_config.clone();
                    config.push_str(&transform_config);
                    config.push_str(&sink_config);

                    let config = config::load_from_str(&config, Some(config::Format::TOML))
                        .expect(&format!("invalid TOML configuration: {}", &config));
                    let mut rt = runtime();
                    let (output_lines, topology) = rt.block_on(async move {
                        let output_lines = CountReceiver::receive_lines(out_addr);
                        let (topology, _crash) = start_topology(config, false).await;
                        wait_for_tcp(in_addr).await;
                        (output_lines, topology)
                    });
                    let lines = lines.clone();
                    (rt, lines, topology, output_lines)
                },
                |(mut rt, lines, topology, output_lines)| {
                    rt.block_on(async move {
                        send_lines(in_addr, lines).await.unwrap();

                        topology.stop().compat().await.unwrap();

                        let output_lines = output_lines.await;

                        #[cfg(debug_assertions)]
                        {
                            assert_eq!(num_lines, output_lines.len());
                            for output_line in output_lines {
                                let actual = serde_json::from_str(output_line);
                                assert_eq!(output, actual);
                            }
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
