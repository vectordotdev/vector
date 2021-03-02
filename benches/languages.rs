use criterion::{criterion_group, criterion_main, BatchSize, Criterion, SamplingMode, Throughput};
use indoc::indoc;
use vector::{
    config,
    test_util::{next_addr, runtime, send_lines, start_topology, wait_for_tcp, CountReceiver},
};

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_add_fields, benchmark_parse_json, benchmark_parse_syslog
);
criterion_main!(benches);

// Add two fields to the event: four=4 and five=5
fn benchmark_add_fields(c: &mut Criterion) {
    let configs: Vec<(&str, &str)> = vec![
        (
            "remap",
            indoc! {r#"
                [transforms.last]
                  type = "remap"
                  inputs = ["in"]
                  source = """
                  .four = 4
                  .five = 5
                  """
            "#},
        ),
        (
            "native",
            indoc! {r#"
                [transforms.last]
                  type = "add_fields"
                  inputs = ["in"]
                  fields.four = 4
                  fields.five = 5
            "#},
        ),
        (
            "lua",
            indoc! {r#"
                [transforms.last]
                  type = "lua"
                  inputs = ["in"]
                  version = "2"
                  source = """
                  function process(event, emit)
                    event.log.four = 4
                    event.log.five = 5
                    emit(event)
                  end
                  """
                  hooks.process = "process"
            "#},
        ),
        (
            "wasm",
            indoc! {r#"
                [transforms.last]
                  type = "wasm"
                  inputs = ["in"]
                  module = "tests/data/wasm/add_fields/target/wasm32-wasi/release/add_fields.wasm"
                  artifact_cache = "target/artifacts/"
                  options.four = 4
                  options.five = 5
            "#},
        ),
    ];

    let input = "";
    let output = serde_json::from_str(r#"{"four": 4, "five": 5 }"#).unwrap();

    benchmark_configs(c, "add_fields", configs, "in", "last", input, &output);
}

fn benchmark_parse_json(c: &mut Criterion) {
    let configs: Vec<(&str, &str)> = vec![
        (
            "remap",
            indoc! {r#"
                [transforms.last]
                  type = "remap"
                  inputs = ["in"]
                  source = """
                  . = parse_json!(string!(.message))
                  """
            "#},
        ),
        (
            "native",
            indoc! {r#"
                [transforms.last]
                  type = "json_parser"
                  inputs = ["in"]
                  field = "message"
            "#},
        ),
        (
            "lua",
            indoc! {r#"
                [transforms.last]
                  type = "lua"
                  inputs = ["in"]
                  version = "2"
                  search_dirs = ["benches/lua_deps"]
                  source = """
                  local json = require "json"

                  function process(event, emit)
                    event.log = json.decode(event.log.message)
                    emit(event)
                  end
                  """
                  hooks.process = "process"
            "#},
        ),
        (
            "wasm",
            indoc! {r#"
                [transforms.last]
                  type = "wasm"
                  inputs = ["in"]
                  module = "tests/data/wasm/parse_json/target/wasm32-wasi/release/parse_json.wasm"
                  artifact_cache = "target/artifacts/"
            "#},
        ),
    ];

    let input =
        r#"{"string":"bar","array":[1,2,3],"boolean":true,"number":47.5,"object":{"key":"value"}}"#;
    let output = serde_json::from_str(r#"{ "array": [1, 2, 3], "boolean": true, "number": 47.5, "object": { "key": "value" }, "string": "bar" }"#).unwrap();

    benchmark_configs(c, "parse_json", configs, "in", "last", input, &output);
}

fn benchmark_parse_syslog(c: &mut Criterion) {
    let configs: Vec<(&str, &str)> = vec![
        (
            "remap",
            indoc! {r#"
                [transforms.last]
                  type = "remap"
                  inputs = ["in"]
                  source = """
                  . = parse_syslog!(string!(.message))
                  """
            "#},
        ),
        (
            "native",
            indoc! {r#"
                [transforms.last]
                  type = "regex_parser"
                  inputs = ["in"]
                  field = "message"
                  patterns = ['^<(?P<priority>\d+)>(?P<version>\d+) (?P<timestamp>\S+) (?P<hostname>\S+) (?P<appname>\S+) (?P<procid>\S+) (?P<msgid>\S+) (?P<sdata>\S+) (?P<message>.+)$']
                  types.appname = "string"
                  types.hostname = "string"
                  types.level = "string"
                  types.message = "string"
                  types.msgid = "string"
                  types.procid = "int"
                  types.timestamp = "timestamp|%Y-%m-%dT%H:%M:%S%.fZ"
            "#},
        ),
        (
            "lua",
            indoc! {r#"
                [transforms.last]
                  type = "lua"
                  inputs = ["in"]
                  version = "2"
                  source = """
                  local function parse_syslog(message)
                    local pattern = "^<(%d+)>(%d+) (%S+) (%S+) (%S+) (%S+) (%S+) (%S+) (.+)$"
                    local priority, version, timestamp, hostname, appname, procid, msgid, sdata, message = string.match(message, pattern)

                    return {priority = priority, version = version, timestamp = timestamp, hostname = hostname, appname = appname, procid = tonumber(procid), msgid = msgid, sdata = sdata, message = message}
                  end

                  function process(event, emit)
                    event.log = parse_syslog(event.log.message)
                    emit(event)
                  end
                  """
                  hooks.process = "process"
            "#},
        ),
        (
            "wasm",
            indoc! {r#"
                [transforms.last]
                  type = "wasm"
                  inputs = ["in"]
                  module = "tests/data/wasm/parse_syslog/target/wasm32-wasi/release/parse_syslog.wasm"
                  artifact_cache = "target/artifacts/"
            "#},
        ),
    ];

    let input = r#"<12>3 2020-12-19T21:48:09.004Z initech.io su 4015 ID81 - TPS report missing cover sheet"#;
    // intentionally leaves out facility and severity as the native implementation, using
    // `regex_parser`, is not able to capture this
    let output = serde_json::from_str(r#"{ "appname": "su", "hostname": "initech.io", "message": "TPS report missing cover sheet", "msgid": "ID81", "procid": 4015, "timestamp": "2020-12-19T21:48:09.004Z" }"#).unwrap();

    benchmark_configs(c, "parse_syslog", configs, "in", "last", input, &output);
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
    output: &serde_json::map::Map<String, serde_json::Value>,
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
        indoc! {r#"
            [sources.{}]
              type = "socket"
              mode = "tcp"
              address = "{}"
        "#},
        input_name, in_addr
    );
    let sink_config = format!(
        indoc! {r#"
            [sinks.out]
              inputs = ["{}"]
              type = "socket"
              mode = "tcp"
              encoding.codec = "json"
              address = "{}"
        "#},
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

                        topology.stop().await;

                        let output_lines = output_lines.await;

                        #[cfg(debug_assertions)]
                        {
                            assert_eq!(num_lines, output_lines.len());
                            for output_line in &output_lines {
                                let actual: serde_json::map::Map<String, serde_json::Value> =
                                    serde_json::from_str(output_line).unwrap();
                                // avoids asserting the actual == expected as the socket trasform
                                // adds dynamic keys like timestamp
                                for (key, value) in output.iter() {
                                    assert_eq!(Some(value), actual.get(key), "for key {}", key,);
                                }
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
