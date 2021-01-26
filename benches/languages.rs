use criterion::{criterion_group, criterion_main, BatchSize, Criterion, SamplingMode, Throughput};
use futures::compat::Future01CompatExt;
use vector::test_util::{
    benchmark_configs, next_addr, runtime, send_lines, start_topology, wait_for_tcp, CountReceiver,
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
