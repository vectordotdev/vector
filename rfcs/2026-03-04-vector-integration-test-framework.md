# RFC - 2026-03-04 - Pipeline Integration Test Framework

A test framework for Vector's internal test suite that allows full pipeline integration tests
to be defined entirely in config and run via `vector test`. The production pipeline config
(sources, transforms, sinks) is left untouched. The `tests` section declares per-test
generators that send data into real sources and listeners that receive data from real sinks.
Real sinks execute their full pipeline —
batching, encoding, compression, retries — against these listeners. Assertions validate what
the listeners actually received, catching serialization bugs, encoding quirks, and
protocol-level issues that unit tests miss.

## Context

- [Component Specification](docs/specs/component.md)
- [Automatic Component Validation RFC](rfcs/2022-11-04-automatic-component-validation.md)
- Existing unit test framework in `src/config/unit_test/`
- Existing topology tests in `src/topology/test/`
- Integration test infrastructure in `vdev/src/testing/`

## Cross cutting concerns

- The existing `[[tests]]` config section and `vector test` CLI command
- Component compliance testing in `src/test_util/components.rs`
- The `cargo vdev int` integration test tooling

## Scope

### In scope

- Per-test `generators` and `listeners` that act as test infrastructure: generators send data
  into real sources, listeners receive data from real sinks
- Real sinks executing their full pipeline against test listeners
- VRL assertions that receive all captured events as an array
- All tests run via `vector test` — no Rust code, no Docker, no external tooling
- Initially for Vector's own internal test suite

### Out of scope

- Replacing the existing Docker-based integration tests for real external services (Kafka,
  Elasticsearch, etc.)
- User-facing config override/patching (future addition)
- Performance/load testing
- Testing Vector's CLI, reload, or signal handling behavior

## Pain

### Integration tests are expensive to write and maintain

Each integration test requires:
1. A `test.yaml` config file with feature flags, environment variables, and matrix definitions
2. A `compose.yaml` with Docker service definitions
3. Docker images that must be built, cached, and maintained
4. A Rust test file that connects to external services via environment variables
5. CI pipeline configuration to trigger the right tests for the right file changes

Even testing a simple "socket source -> remap -> HTTP sink" pipeline requires spinning up
Docker containers, managing network configuration, and dealing with service readiness polling.

### The `[[tests]]` framework is transform-only

The existing unit test framework (`src/config/unit_test/`) strips all sources and sinks and
replaces them with synthetic components. This only tests transforms. Real sinks — with their
encoding, batching, compression, and protocol-specific serialization — are never exercised.

### Unit tests miss sink-level bugs

Sink bugs often live in the serialization and protocol layers:
- An HTTP sink may produce invalid JSON when a field contains special characters
- A codec may drop fields during encoding
- Compression may interact badly with certain payload sizes
- Batch size limits may split events in unexpected ways

None of these are caught by transform-only unit tests. They require the real sink to actually
run, serialize events, and send them over the wire to a server that can inspect the result.

### Existing topology tests require Rust code

The topology tests in `src/topology/test/end_to_end.rs` demonstrate that full pipeline testing
is possible — load a TOML config, start a topology, spin up a mock HTTP server, verify output.
But this requires writing Rust, constructing mock servers manually, and is not reusable.

## Proposal

### User Experience

The production pipeline config (sources, transforms, sinks) is written normally with real
addresses and real component types. The `tests` section declares per-test `generators` that feed data into sources and
`listeners` that capture data from sinks.

#### Example 1: test an HTTP sink end-to-end

```yaml
# tests/pipelines/http_sink.yml

sources:
  socket:
    type: socket
    address: 0.0.0.0:9000

transforms:
  parse:
    inputs: ["socket"]
    type: remap
    source: |
      .parsed = true
      .severity = upcase!(.level)
      del(.level)

sinks:
  http_out:
    inputs: ["parse"]
    type: http
    encoding:
      codec: json
    uri: http://0.0.0.0:9001/

tests:
  - name: "transforms and sends two events"
    generators:
      gen:
        type: socket
        address: 0.0.0.0:9000
        events:
          - source: '{ "message": "hello world", "level": "info" }'
          - source: '{ "message": "something failed", "level": "error" }'

    listeners:
      out:
        type: http
        port: 9001
        decoding:
          codec: json

    outputs:
      - extract_from: out
        conditions:
          - type: vrl
            source: |
              assert!(is_array(.))
              assert_eq!(length(.), 2)

              assert_eq!(.[0].message, "hello world")
              assert_eq!(.[0].severity, "INFO")
              assert_eq!(.[0].parsed, true)
              assert!(!exists(.[0].level))

              assert_eq!(.[1].message, "something failed")
              assert_eq!(.[1].severity, "ERROR")
```

Run it:
```
$ vector test tests/pipelines/http_sink.yml
Running tests
test transforms and sends two events ... passed
```

What happens under the hood:
1. The framework starts the test infrastructure: `gen` (a socket generator) and `out` (an
   HTTP listener on port 9001)
2. The full topology starts: real `socket` source on port 9000, real `remap` transform,
   real `http` sink posting to `http://0.0.0.0:9001/`
3. `gen` connects to the socket source on port 9000 and sends the test events
4. Events flow through the real remap transform
5. The real HTTP sink batches, encodes to JSON, and POSTs to `http://0.0.0.0:9001/`
6. `out` receives the HTTP requests, decodes the JSON bodies
7. All decoded events are collected into an array
8. VRL assertions run against that array

#### Example 2: test a `route` transform with two sinks

```yaml
sources:
  socket:
    type: socket
    address: 0.0.0.0:9000

transforms:
  router:
    inputs: ["socket"]
    type: route
    route:
      prod:
        type: vrl
        source: '.env == "prod"'

sinks:
  prod_sink:
    inputs: ["router.prod"]
    type: http
    encoding:
      codec: json
    uri: http://0.0.0.0:9001/

  default_sink:
    inputs: ["router._default"]
    type: http
    encoding:
      codec: json
    uri: http://0.0.0.0:9002/

tests:
  - name: "routes events to correct endpoints"
    generators:
      gen:
        type: socket
        address: 0.0.0.0:9000
        events:
          - source: '{ "message": "event 1", "env": "prod" }'
          - source: '{ "message": "event 2", "env": "staging" }'

    listeners:
      prod_receiver:
        type: http
        port: 9001
        decoding:
          codec: json

      default_receiver:
        type: http
        port: 9002
        decoding:
          codec: json

    outputs:
      - extract_from: prod_receiver
        conditions:
          - type: vrl
            source: |
              assert_eq!(length(.), 1)
              assert_eq!(.[0].env, "prod")

      - extract_from: default_receiver
        conditions:
          - type: vrl
            source: |
              assert_eq!(length(.), 1)
              assert_eq!(.[0].env, "staging")
```

#### Example 3: test error handling with a failing server

```yaml
sources:
  socket:
    type: socket
    address: 0.0.0.0:9000

sinks:
  http_out:
    inputs: ["socket"]
    type: http
    encoding:
      codec: json
    uri: http://0.0.0.0:9001/
    request:
      retry_max_duration_secs: 1

tests:
  - name: "retries on server error"
    generators:
      gen:
        type: socket
        address: 0.0.0.0:9000
        events:
          - source: '{ "message": "test" }'

    listeners:
      out:
        type: http
        port: 9001
        status_code: 500
        decoding:
          codec: json

    outputs:
      - extract_from: out
        conditions:
          - type: vrl
            source: |
              # Server received multiple attempts due to retries
              assert!(length(.) >= 2)
```

#### Example 4: test with ndjson encoding and compression

```yaml
sources:
  socket:
    type: socket
    address: 0.0.0.0:9000

sinks:
  http_out:
    inputs: ["socket"]
    type: http
    encoding:
      codec: json
    framing:
      method: newline_delimited
    compression: gzip
    uri: http://0.0.0.0:9001/

tests:
  - name: "gzip compressed ndjson arrives correctly"
    generators:
      gen:
        type: socket
        address: 0.0.0.0:9000
        events:
          - source: '{ "msg": "line 1" }'
          - source: '{ "msg": "line 2" }'

    listeners:
      out:
        type: http
        port: 9001
        decompression: gzip
        decoding:
          codec: json

    outputs:
      - extract_from: out
        conditions:
          - type: vrl
            source: |
              assert_eq!(length(.), 2)
              assert_eq!(.[0].msg, "line 1")
              assert_eq!(.[1].msg, "line 2")
```

### Implementation

#### Generator and listener types

Generators are declared per-test under `tests[].generators`. Each generator sends data into a
real source.

| Type | Description | Connects to |
|------|-------------|-------------|
| `socket` | Sends events over TCP to a socket source | `socket` source address |
| `http` | Sends HTTP requests to an HTTP source | `http_server` source address |

Listeners are declared per-test under `tests[].listeners`. Each listener receives data from a
real sink.

| Type | Description | Receives from |
|------|-------------|---------------|
| `http` | HTTP server that captures request bodies | `http` sink, Splunk HEC sink, etc. |
| `tcp` | TCP server that captures newline-delimited data | `socket` sink |

**Common listener config fields:**

```yaml
type: http
port: 9001                  # port to listen on
status_code: 200            # HTTP response status (default 200)
decompression: gzip         # decompress bodies (none, gzip, zstd)
decoding:
  codec: json               # parse bodies (json, text, etc.)
```

**Common generator config fields:**

```yaml
type: socket
address: 0.0.0.0:9000       # address to connect to (matches the source)
events:                      # events to send
  - source: '{ "message": "hello" }'
  - value: "raw log line"
```

Event definitions in generators support the same types as existing `[[tests]]` inputs:
`source` (VRL), `value` (raw string), `log_fields` (structured), and metric definitions.
The VRL compilation logic is extracted from the existing `build_input_event()` in
`src/config/unit_test/mod.rs:606-666` into a shared module.

#### Assertion model

VRL assertions receive `.` as an **array of all events** captured by a listener. This allows:
- Count checks: `assert_eq!(length(.), 2)`
- Index-based access: `.[0].message`, `.[1].severity`
- Iteration: `for_each(.) -> |_i, e| { assert!(exists(e.message)) }`
- Aggregate checks: `assert!(all(.) -> |_i, e| { exists(e.timestamp) })`

This differs from existing `[[tests]]` where conditions run once per event. The array model
is more powerful for pipeline tests because you need to verify event count, ordering, and
cross-event relationships.

#### Lifecycle

For each test:

1. **Start listeners** — bind to their configured ports, ready to receive
2. **Start topology** — sources, transforms, and sinks start normally
3. **Wait for sources to be ready** — `wait_for_tcp()` on source addresses
4. **Run generators** — connect to sources and send events
5. **Wait for pipeline to drain** — generators signal completion, framework waits for sinks
   to flush (topology shutdown + grace period)
6. **Collect from listeners** — each listener returns its captured events as an array
7. **Run assertions** — VRL conditions execute against collected arrays
8. **Teardown** — stop topology and listeners

```rust
pub struct PipelineTest {
    pub name: String,
    config: Config,
    pieces: TopologyPieces,
    generators: Vec<Box<dyn TestGenerator>>,
    listeners: HashMap<String, Box<dyn TestListener>>,
    outputs: Vec<TestOutput>,
}

impl PipelineTest {
    pub async fn run(self) -> UnitTestResult {
        // 1. Start listeners
        for listener in self.listeners.values() {
            listener.start().await;
        }

        // 2. Start topology
        let diff = config::ConfigDiff::initial(&self.config);
        let (topology, _) = RunningTopology::start_validated(
            self.config, diff, self.pieces
        ).await.unwrap();

        // 3. Wait for sources
        for generator in &self.generators {
            wait_for_tcp(generator.target_address()).await;
        }

        // 4. Run generators
        for generator in &self.generators {
            generator.send().await;
        }

        // 5. Stop topology — flushes all buffered events through sinks before returning.
        //    No grace period needed; topology.stop() is the correct synchronization point.
        topology.stop().await;

        // 6. Collect from listeners
        let mut collected: HashMap<String, Vec<Event>> = HashMap::new();
        for (name, listener) in &self.listeners {
            collected.insert(name.clone(), listener.collect().await);
        }

        // 7. Run assertions
        let mut errors = Vec::new();
        for output in &self.outputs {
            let events = collected.get(&output.extract_from)
                .unwrap_or(&Vec::new());

            // Build a VRL Value::Array from collected events
            let array_value = events_to_vrl_array(events);

            if let Some(conditions) = &output.conditions {
                for (i, condition) in conditions.iter().enumerate() {
                    if let Err(e) = run_vrl_assertion(condition, &array_value) {
                        errors.push(format!(
                            "'{}', condition[{}]: {}",
                            output.extract_from, i, e
                        ));
                    }
                }
            }
        }

        UnitTestResult { errors }
    }
}
```

#### Generator and listener traits

```rust
#[async_trait]
pub trait TestGenerator: Send + Sync {
    /// The address this generator sends to
    fn target_address(&self) -> SocketAddr;

    /// Send all configured events to the target
    async fn send(&self) -> Result<(), String>;
}

#[async_trait]
pub trait TestListener: Send + Sync {
    /// Start listening (bind to port)
    async fn start(&mut self) -> Result<(), String>;

    /// Collect all received data as events
    async fn collect(&mut self) -> Vec<Event>;
}
```

#### HTTP listener implementation

`HttpListener` wraps `build_test_server_generic()` from `src/sinks/util/test.rs:69`, which
already handles hyper server setup, async body capture, and `Trigger`/`Tripwire`-based graceful
shutdown. `HttpListener` only adds decompression and decoding on top of the captured bytes.

```rust
pub struct HttpListener {
    addr: SocketAddr,
    status_code: StatusCode,
    decompression: Option<Decompression>,
    decoding: DecodingConfig,
    // populated after start()
    rx: Option<mpsc::Receiver<(Parts, Bytes)>>,
    trigger: Option<Trigger>,
}

#[async_trait]
impl TestListener for HttpListener {
    async fn start(&mut self) -> Result<(), String> {
        let status = self.status_code;
        let (rx, trigger, server) = build_test_server_generic(self.addr, move || {
            Response::builder().status(status).body(Body::empty()).unwrap()
        });
        tokio::spawn(server);
        self.rx = Some(rx);
        self.trigger = Some(trigger);
        wait_for_tcp(self.addr).await;
        Ok(())
    }

    async fn collect(&mut self) -> Vec<Event> {
        drop(self.trigger.take()); // signal shutdown; drains the channel
        let bodies: Vec<Bytes> = self.rx.take().unwrap()
            .collect::<Vec<_>>().await
            .into_iter().map(|(_, b)| b).collect();
        decode_bodies(bodies, &self.decompression, &self.decoding)
    }
}
```

#### Socket generator implementation

`SocketGenerator` wraps `send_lines()` from `src/test_util/mod.rs:137`, which already handles
TCP connect, `LinesCodec` framing, and clean shutdown.

```rust
pub struct SocketGenerator {
    address: SocketAddr,
    events: Vec<Event>,
}

#[async_trait]
impl TestGenerator for SocketGenerator {
    fn target_address(&self) -> SocketAddr {
        self.address
    }

    async fn send(&self) -> Result<(), String> {
        let lines = self.events.iter()
            .map(|e| serde_json::to_string(e.as_log()).unwrap())
            .collect::<Vec<_>>();
        send_lines(self.address, lines).await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
```

#### TCP listener implementation

`TcpListener` wraps `CountReceiver::receive_lines()` from `src/test_util/mod.rs:641`, which
already binds a TCP port, frames input with `LinesCodec`, and collects newline-delimited strings.

```rust
pub struct TcpListener {
    addr: SocketAddr,
    receiver: Option<CountReceiver<String>>,
}

#[async_trait]
impl TestListener for TcpListener {
    async fn start(&mut self) -> Result<(), String> {
        // receive_lines binds the port immediately
        self.receiver = Some(CountReceiver::receive_lines(self.addr));
        wait_for_tcp(self.addr).await;
        Ok(())
    }

    async fn collect(&mut self) -> Vec<Event> {
        self.receiver.take().unwrap().await
            .into_iter()
            .map(|line| Event::Log(LogEvent::from_str_legacy(line)))
            .collect()
    }
}
```

#### Extending the test framework

The existing `vector test` CLI (`src/unit_test.rs:139`) calls
`config::build_unit_tests_main()`. This function dispatches to the pipeline test path when
it detects `tests[].generators` or `tests[].listeners`:

```rust
pub async fn build_unit_tests_main(
    paths: &[ConfigPath],
    signal_handler: &mut signal::SignalHandler,
) -> Result<Vec<Box<dyn RunnableTest>>, Vec<String>> {
    let config_builder = /* load config */;

    if has_pipeline_test_components(&config_builder) {
        build_pipeline_tests(config_builder).await
    } else {
        build_unit_tests(config_builder).await  // existing path
    }
}
```

#### How it differs from existing `[[tests]]`

| Aspect | `[[tests]]` unit tests | Pipeline integration tests |
|--------|----------------------|----------------------------|
| Scope | Transforms only | Full pipeline including sinks |
| Sources | Stripped and replaced | Real — generators send data into them |
| Transforms | Kept (relevant subset) | Kept (all) |
| Sinks | Stripped and replaced | Real — full execution against listeners |
| What is tested | Transform logic | Transform logic + sink encoding, batching, compression |
| Test infra | Synthetic components replace pipeline | Generators and listeners wrap the real pipeline |
| Assertion input | Single event per condition | Array of all captured events |
| Network I/O | None | Loopback (generators -> sources, sinks -> listeners) |
| Execution | `vector test` | `vector test` (same) |
| Rust code needed | No | No |

#### File organization

```
src/test_util/
├── pipeline_test/
│   ├── mod.rs              # build_pipeline_tests(), PipelineTest
│   ├── generators/
│   │   ├── mod.rs           # TestGenerator trait
│   │   ├── socket.rs        # SocketGenerator
│   │   └── http.rs          # HttpGenerator
│   ├── listeners/
│   │   ├── mod.rs           # TestListener trait
│   │   ├── http.rs          # HttpListener
│   │   └── tcp.rs           # TcpListener
│   └── assertions.rs        # VRL array assertion runner
├── event_builder.rs         # Shared: build Event from VRL/raw/log/metric defs
├── mod.rs                   # existing (add pub mod pipeline_test)
└── ...
```

Test config files:

```
tests/
├── behavior/
│   ├── transforms/         # existing [[tests]] configs
│   └── pipelines/          # new pipeline integration tests
│       ├── http_sink.yml
│       ├── route_multi_output.yml
│       ├── ndjson_encoding.yml
│       └── compression.yml
```

## Rationale

### Why is this change worth it?

- **Catches real bugs**: The full sink pipeline executes — encoding, batching, compression,
  HTTP requests. The listener receives exactly what a real service would receive. Serialization
  bugs, encoding edge cases, and protocol issues are caught.
- **Zero Rust code**: Tests are pure config. A developer adding a new sink can write pipeline
  tests without knowing the test infrastructure internals.
- **No Docker**: Generators and listeners run in-process on loopback. No containers, no images,
  no compose files.
- **Real pipeline, real config**: The sources, transforms, and sinks section is a real
  production-like config. The test infrastructure wraps it, it doesn't replace it.
- **Array assertions**: VRL conditions receive all events as an array, enabling count checks,
  ordering verification, and cross-event assertions.

### What is the impact of not doing this?

Developers continue to either write transform-only unit tests that miss sink bugs, or set up
the full Docker integration test infrastructure. Many sink bugs ship because the barrier to
testing is too high.

### How does this position us for success in the future?

- Every new sink can ship with pipeline tests from day one
- Sink encoding/compression regressions are caught in CI without Docker
- Foundation for user-facing config testing in the future

## Drawbacks

- **Loopback networking**: Tests use real TCP connections on loopback. Port conflicts are
  possible if tests run in parallel with hardcoded ports. Mitigated initially by using unique
  ports per test file; eliminated in Part 2 via automatic port allocation.
- **Sink flush timing**: After generators finish sending, sinks may still be batching
  internally. The framework must wait for sinks to flush before collecting from listeners.
  This introduces a timing dependency.
- **Payload parsing in listeners**: Listeners must decompress and decode payloads to produce
  events for assertion. Initially only JSON/ndjson is supported; other codecs are added as
  needed.
- **Mock fidelity**: Listeners don't replicate real service behavior (authentication, rate
  limiting, protocol negotiation). Docker integration tests remain necessary for testing
  against real services.

## Prior Art

- **Vector's `[[tests]]` config section** (`src/config/unit_test/`): Transform-only testing
  with VRL assertions. This RFC extends the concept to full pipelines.
- **Vector's `end_to_end.rs`** (`src/topology/test/end_to_end.rs`): Loads a TOML config,
  starts a topology with real HTTP source/sink, spins up a mock HTTP server, and verifies
  output. This is the closest existing prior art — this RFC makes the same pattern declarative.
- **Vector's `spawn_blackhole_http_server`** (`src/test_util/http.rs`): Existing in-process
  mock HTTP server. The HTTP listener builds on the same `hyper` pattern with structured capture.
- **Vector's `build_test_server_generic()`** (`src/sinks/util/test.rs:69`): Full hyper server
  with `Trigger`/`Tripwire` shutdown and MPSC request capture. `HttpListener` wraps this directly.
- **Vector's `CountReceiver::receive_lines()`** (`src/test_util/mod.rs:641`): TCP listener with
  `LinesCodec` framing and async collection. `TcpListener` wraps this directly.
- **Vector's `send_lines()`** (`src/test_util/mod.rs:137`): TCP client with `LinesCodec` framing
  and clean shutdown. `SocketGenerator` wraps this directly.
- **Vector's `build_input_event()`** (`src/config/unit_test/mod.rs:606`): Builds events from
  VRL/raw/log/metric definitions. Reused by generators.
- **HTTP sink tests** (`src/sinks/http/tests.rs`): Existing Rust tests that build an HTTP
  sink, spawn a mock server, and verify output. This RFC makes the same flow declarative.

## Alternatives

### Replace sinks with in-memory collectors

An earlier iteration proposed replacing real sinks with collectors that capture events before
serialization. This was rejected because it doesn't test the sink's encoding, batching,
compression, or protocol behavior — which is the whole point.

### Use `{{ template }}` addresses instead of hardcoded ports

The framework could auto-allocate ports and inject them via templates. Deferred to Part 2 —
the initial implementation uses hardcoded ports for simplicity. Auto-allocation is the intended
end state and is designed to be a non-breaking addition.

### Require Rust code for pipeline tests

The topology `end_to_end.rs` pattern works but requires Rust. This limits who can write tests
and prevents test configs from being self-documenting.

### Do nothing

Sink-level bugs continue to reach production. The barrier to testing stays high.

## Outstanding Questions

- How should the framework handle sink flush timing? Options: fixed grace period, wait for
  all sinks to report idle, or configurable timeout per test.
- Should listeners support request-level assertions (HTTP headers, method, path) in addition
  to body content?
- Should listeners support response sequences (`[200, 200, 500, 200]`) for testing retry
  behavior, or is a single `status_code` sufficient initially?
- Should port allocation be automatic (random ports with template injection) or manual
  (hardcoded ports as in the examples)? See Part 2 for the proposed template approach.
- How should non-JSON payloads be handled by listeners? Options: raw byte comparison,
  codec-specific decoders, or pluggable parsers.

## Plan Of Attack

- [ ] Extract `build_input_event()` from `src/config/unit_test/mod.rs` into a shared
  `src/test_util/event_builder.rs`
- [ ] Implement `TestGenerator` trait and `SocketGenerator`
- [ ] Implement `TestListener` trait and `HttpListener` with decompression and JSON decoding
- [ ] Implement `tests[].generators` and `tests[].listeners` config parsing
- [ ] Implement VRL array assertion runner (pass `.` as array of events)
- [ ] Implement `build_pipeline_tests()` alongside `build_unit_tests()`
- [ ] Extend `build_unit_tests_main()` to detect and dispatch pipeline tests
- [ ] Write first pipeline test: socket source -> remap -> HTTP sink -> HTTP listener
- [ ] Write pipeline test for route transform with two HTTP sinks
- [ ] Write pipeline test for gzip compression + ndjson encoding
- [ ] Implement `HttpGenerator` for testing HTTP sources
- [ ] Implement `TcpListener` for testing socket sinks
- [ ] Add pipeline test examples to `tests/behavior/pipelines/`
- [ ] Add documentation in `docs/DEVELOPING.md`

## Future Improvements

- **Response sequences**: `status_codes: [200, 200, 500, 200]` for testing retry logic
- **Request-level assertions**: Assert on HTTP method, path, headers, content-encoding
- **gRPC listener**: For testing OpenTelemetry sinks
- **User-facing config override**: Auto-patch production configs to route sinks to listeners
  during `vector test`
- **Snapshot testing**: Compare listener captures against golden files
- **Codec registry**: Pluggable decoders for Avro, Protobuf, MsgPack
- **Metrics assertions**: Assert on Vector's internal metrics alongside event output
- **Generator from file**: Load events from a JSON/CSV fixture file

---

## Part 2: Automatic Port Allocation

The initial implementation uses hardcoded ports (e.g. `0.0.0.0:9000`, `0.0.0.0:9001`).
This works for sequential test runs but breaks parallel execution. Part 2 replaces hardcoded
ports with auto-allocated addresses injected via config template placeholders.

### Proposed config syntax

```yaml
sources:
  socket:
    type: socket
    address: "{{test.gen.gen}}"            # resolved to 127.0.0.1:<auto-port>

sinks:
  http_out:
    inputs: ["parse"]
    type: http
    encoding:
      codec: json
    uri: "http://{{test.listener.out}}/"   # resolved to 127.0.0.1:<auto-port>

tests:
  - name: "transforms and sends two events"
    generators:
      gen:                                 # name matches {{test.gen.gen}}
        type: socket
        events:
          - source: '{ "message": "hello world", "level": "info" }'

    listeners:
      out:                                 # name matches {{test.listener.out}}
        type: http
        decoding:
          codec: json
```

Generators no longer carry an `address` field — the address is derived from the placeholder
bound to their name. Listeners no longer carry a `port` field for the same reason.

### How it works

1. After loading raw config text and before parsing the topology, the framework scans for
   `{{test.gen.<name>}}` and `{{test.listener.<name>}}` placeholders.
2. For each unique name, it calls `next_addr()` from `src/test_util/addr.rs` to allocate a
   random loopback port. The returned `PortGuard` is held alive for the duration of the test,
   preventing races with concurrent tests.
3. All occurrences of each placeholder are string-substituted with the allocated
   `127.0.0.1:<port>` before the config is handed to the topology builder.
4. The resolved addresses are passed to the corresponding generators and listeners so they
   connect and bind to the same ports.

### Plan of Attack (Part 2)

- [ ] Implement `resolve_test_addresses(raw_config: &str) -> (String, AddressMap)` that scans
  for placeholders, allocates ports via `next_addr()`, and returns the substituted config text
  alongside a map of name → `SocketAddr`
- [ ] Thread `AddressMap` through `build_pipeline_tests()` so generators and listeners receive
  their allocated addresses
- [ ] Remove `address` field from generator config; remove `port` field from listener config
- [ ] Update `tests/behavior/pipelines/` examples to use template syntax
- [ ] Verify parallel `vector test` invocations do not conflict
