use criterion::{criterion_group, criterion_main, BatchSize, Criterion, SamplingMode, Throughput};
use futures::{compat::Future01CompatExt, stream, StreamExt};
use vector::{
    config, sinks, sources,
    test_util::{next_addr, runtime, send_lines, start_topology, wait_for_tcp, CountReceiver},
    transforms,
};

fn benchmark_vrl_announcement(c: &mut Criterion) {
    vector::test_util::trace_init();

    let num_lines = 10_000;
    let in_addr = next_addr();
    let out_addr = next_addr();

    let configs: Vec<(&str, config::ConfigBuilder)> = vec![
        ("remap", {
            let mut config = config::Config::builder();
            config.add_source(
                "in",
                sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
            );
            config.add_transform(
                "parser",
                &["in"],
                toml::from_str::<transforms::remap::RemapConfig>(
                    r#"
source = """
    . = parse_syslog(.message)
    .severity = "info"
    .id = uuid_v4()
    .timestamp = to_int(.timestamp)
"""
"#,
                )
                .unwrap(),
            );
            config.add_sink(
                "out",
                &["parser"],
                sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
            );

            config
        }),
        ("native_and_lua", {
            let mut config = config::Config::builder();
            config.add_source(
                "in",
                sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
            );
            config.add_transform(
                "parse_syslog",
                &["in"],
                toml::from_str::<transforms::regex_parser::RegexParserConfig>(
                    r#"
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
                ).unwrap(),
            );
            config.add_transform(
                "add_fields",
                &["parse_syslog"],
                toml::from_str::<transforms::add_fields::AddFieldsConfig>(
                    r#"
fields.severity = "info"
                    "#,
                )
                .unwrap(),
            );
            config.add_transform(
                "lua_parser",
                &["add_fields"],
                toml::from_str::<transforms::lua::LuaConfig>(
                    r#"
version = "2"
source = """
local random = math.random

local function uuid()
    local template ='xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'
    return string.gsub(template, '[xy]', function (c)
        local v = (c == 'x') and random(0, 0xf) or random(8, 0xb)
        return string.format('%x', v)
    end)
end

function process(event, emit)
  event.log.id = uuid()
  event.log.timestamp = os.time(event.log.timestamp)
  emit(event)
end
"""
hooks.process = "process"
"#,
                )
                .unwrap(),
            );
            config.add_sink(
                "out",
                &["lua_parser"],
                sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
            );

            config
        }),
        ("lua", {
            let mut config = config::Config::builder();
            config.add_source(
                "in",
                sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
            );
            config.add_transform(
                    "parser",
                    &["in"],
                toml::from_str::<transforms::lua::LuaConfig>(r#"
version = "2"
source = """
local random = math.random

local function uuid()
    local template ='xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'
    return string.gsub(template, '[xy]', function (c)
        local v = (c == 'x') and random(0, 0xf) or random(8, 0xb)
        return string.format('%x', v)
    end)
end

local function date_to_timestamp(date)
  local pattern = "^(%d+)-(%d+)-(%d+)T(%d+):(%d+):(%d+)%.(%d+)Z$"
  local year, month, day, hour, minute, seconds = string.match(date, pattern)

  return os.time({year = year, month = month, day = day, hour = hour, min = minute, sec = seconds})
end

local function parse_syslog(message)
  local pattern = "^<(%d+)>(%d+) (%S+) (%S+) (%S+) (%S+) (%S+) (%S+) (.+)$"
  local priority, version, timestamp, host, appname, procid, msgid, sdata, message = string.match(message, pattern)

  return {priority = priority, version = version, timestamp = timestamp, host = host, appname = appname, procid = procid, msgid = msgid, sdata = sdata, message = message}
end

function process(event, emit)
  event.log = parse_syslog(event.log.message)
  event.log.severity = "info"
  event.log.id = uuid()
  event.log.timestamp = date_to_timestamp(event.log.timestamp)
  emit(event)
end
"""
hooks.process = "process"
"#

).unwrap());
            config.add_sink(
                "out",
                &["parser"],
                sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
            );

            config
        }),
        ("wasm", {
            let mut config = config::Config::builder();
            config.add_source(
                "in",
                sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
            );
            config.add_transform(
                "parser",
                &["in"],
                toml::from_str::<transforms::wasm::WasmConfig>(
                    r#"
module = "tests/data/wasm/vrl_announcement_example/target/wasm32-wasi/release/vrl_announcement_example.wasm"
artifact_cache = "target/artifacts/"
"#,
                )
                .unwrap(),
            );
            config.add_sink(
                "out",
                &["parser"],
                sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
            );

            config
        }),
    ];

    let mut group = c.benchmark_group("language/vrl_announcement");
    group.sampling_mode(SamplingMode::Flat);

    for (name, config) in configs.iter() {
        group.throughput(Throughput::Elements(num_lines as u64));
        group.bench_function(format!("language/vrl_announcement/{}", name), |b| {
            b.iter_batched(
                || {
                    let config = config.clone();

                    let mut rt = runtime();
                    let (output_lines, topology) = rt.block_on(async move {
                        let output_lines = CountReceiver::receive_lines(out_addr);
                        let (topology, _crash) =
                            start_topology(config.build().unwrap(), false).await;
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

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_vrl_announcement
);
criterion_main!(benches);
