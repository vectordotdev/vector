# Implementation Sketch: Pipeline Integration Test Framework

Implementation details for [RFC 2026-03-04](./2026-03-04-vector-integration-test-framework.md).

---

## RunnableTest trait

`build_unit_tests_main()` currently returns `Vec<UnitTest>`. The signature changes to
`Vec<Box<dyn RunnableTest>>` so both `UnitTest` and `PipelineTest` can be driven by the same
runner loop in `src/unit_test.rs`.

```rust
#[async_trait]
pub trait RunnableTest: Send {
    fn name(&self) -> &str;
    async fn run(self: Box<Self>) -> UnitTestResult;
}

impl RunnableTest for UnitTest {
    fn name(&self) -> &str { &self.name }
    async fn run(self: Box<Self>) -> UnitTestResult { (*self).run().await }
}

impl RunnableTest for PipelineTest {
    fn name(&self) -> &str { &self.name }
    async fn run(self: Box<Self>) -> UnitTestResult { (*self).run().await }
}
```

## Config schema

`TestGeneratorConfig` and `TestListenerConfig` are discriminated unions on `type`, parsed from
the `tests[].generators` and `tests[].listeners` maps. They are added to the existing
`TestDefinition` struct alongside `inputs`, `outputs`, and `no_outputs_from`.

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TestGeneratorConfig {
    Socket {
        address: SocketAddr,
        events: Vec<InputDefinition>,
    },
    Http {
        address: SocketAddr,
        events: Vec<InputDefinition>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TestListenerConfig {
    Http {
        port: u16,
        #[serde(default = "default_status_200")]
        status_code: u16,
        #[serde(default)]
        decompression: Option<DecompressionConfig>,
        decoding: DecodingConfig,
    },
    Tcp {
        port: u16,
    },
}

/// Extended TestDefinition — new fields added alongside existing ones
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TestDefinition<T = OutputCheck> {
    pub name: String,
    // existing fields
    pub inputs: Vec<TestInput>,
    pub outputs: Vec<T>,
    pub no_outputs_from: Vec<String>,
    // new fields
    #[serde(default)]
    pub generators: IndexMap<String, TestGeneratorConfig>,
    #[serde(default)]
    pub listeners: IndexMap<String, TestListenerConfig>,
}
```

`InputDefinition` is the existing per-event type used by `build_input_event()` in
`src/config/unit_test/mod.rs:606`. The `source:` field in generator event definitions maps
directly to `InputDefinition::vrl { source }` — it is VRL source code whose return value
becomes the event. This matches the existing `type: vrl` / `source:` convention in
`[[tests]]` inputs. No new field names are introduced.

## Dispatch

A config file is either all pipeline tests or all unit tests. Mixed configs are an error.

```rust
pub async fn build_unit_tests_main(
    paths: &[ConfigPath],
    signal_handler: &mut signal::SignalHandler,
) -> Result<Vec<Box<dyn RunnableTest>>, Vec<String>> {
    let config_builder = /* load config */;

    match classify_test_config(&config_builder) {
        TestConfigKind::Pipeline => build_pipeline_tests(config_builder).await,
        TestConfigKind::Unit => build_unit_tests(config_builder).await,
        TestConfigKind::Mixed => Err(vec![
            "mixed pipeline and unit test definitions are not supported in the same file \
             — split into separate files".to_string()
        ]),
    }
}
```

`classify_test_config` returns `Pipeline` if any test has a non-empty `generators` or
`listeners` map, `Unit` if none do, and `Mixed` if some tests have them and some don't.

## PipelineTest

```rust
pub struct PipelineTest {
    pub name: String,
    config: Config,
    pieces: TopologyPieces,
    generators: Vec<Box<dyn TestGenerator>>,
    listeners: HashMap<String, Box<dyn TestListener>>,
    outputs: Vec<TestOutput>,
    // Kept alive to hold port reservations for the duration of the test (Part 2)
    _port_guards: Vec<PortGuard>,
}

const TEST_TIMEOUT: Duration = Duration::from_secs(30);

impl PipelineTest {
    pub async fn run(self) -> UnitTestResult {
        match tokio::time::timeout(TEST_TIMEOUT, self.run_inner()).await {
            Ok(result) => result,
            Err(_) => UnitTestResult {
                errors: vec![format!(
                    "test '{}' timed out after {}s",
                    self.name,
                    TEST_TIMEOUT.as_secs()
                )],
            },
        }
    }

    async fn run_inner(mut self) -> UnitTestResult {
        // 1. Start listeners
        for (name, listener) in self.listeners.iter_mut() {
            if let Err(e) = listener.start().await {
                return UnitTestResult {
                    errors: vec![format!("failed to start listener '{}': {}", name, e)],
                };
            }
        }

        // 2. Start topology
        let diff = config::ConfigDiff::initial(&self.config);
        let (topology, _) = match RunningTopology::start_validated(
            self.config, diff, self.pieces
        ).await {
            Some(result) => result,
            None => return UnitTestResult {
                errors: vec!["topology failed to start (config validation error)".to_string()],
            },
        };

        // 3. Wait for sources
        for generator in &self.generators {
            if let Err(e) = wait_for_tcp_timeout(generator.target_address(), TEST_TIMEOUT).await {
                return UnitTestResult { errors: vec![e] };
            }
        }

        // 4. Run generators
        for generator in &self.generators {
            if let Err(e) = generator.send().await {
                return UnitTestResult {
                    errors: vec![format!("generator error: {}", e)],
                };
            }
        }

        // 5. Stop topology — flushes all buffered events through sinks before returning.
        //    The outer timeout in run() guards against a stuck sink (e.g., retrying against
        //    a 500 listener indefinitely), since graceful_shutdown_duration defaults to None
        //    (Box::pin(future::pending())) when Config is constructed by build_pipeline_tests.
        topology.stop().await;

        // 6. Collect from listeners
        let mut collected: HashMap<String, Vec<Event>> = HashMap::new();
        for (name, listener) in &mut self.listeners {
            collected.insert(name.clone(), listener.collect().await);
        }

        // 7. Run assertions
        let mut errors = Vec::new();
        for output in &self.outputs {
            let events = collected.get(&output.extract_from).unwrap_or(&Vec::new());
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

## Generator and listener traits

```rust
#[async_trait]
pub trait TestGenerator: Send + Sync {
    fn target_address(&self) -> SocketAddr;
    async fn send(&self) -> Result<(), String>;
}

#[async_trait]
pub trait TestListener: Send + Sync {
    async fn start(&mut self) -> Result<(), String>;
    async fn collect(&mut self) -> Vec<Event>;
}
```

## HttpListener

`build_test_server_generic()` at `src/sinks/util/test.rs:69` only forwards request bodies to
its channel when the response status is 2xx. For error-status tests (e.g., `status_code: 500`),
no bodies would be captured even though the sink is retrying — making `length(.) >= 2`
silently fail with a misleading assertion error. `HttpListener` therefore builds its own hyper
server that captures the body on every request regardless of the configured response status.
It borrows the `Trigger`/`Tripwire` shutdown pattern from `build_test_server_generic()` as a
reference.

```rust
pub struct HttpListener {
    addr: SocketAddr,
    status_code: StatusCode,
    decompression: Option<Decompression>,
    decoding: DecodingConfig,
    rx: Option<mpsc::Receiver<(Parts, Bytes)>>,
    trigger: Option<Trigger>,
}

#[async_trait]
impl TestListener for HttpListener {
    async fn start(&mut self) -> Result<(), String> {
        let status = self.status_code;
        let (tx, rx) = mpsc::channel(128);
        let (trigger, tripwire) = Tripwire::new();

        let server = build_capturing_http_server(self.addr, status, tx, tripwire);
        tokio::spawn(server);
        self.rx = Some(rx);
        self.trigger = Some(trigger);

        wait_for_tcp_timeout(self.addr, Duration::from_secs(5)).await
            .map_err(|e| format!("listener '{}' failed to bind: {}", self.addr, e))
    }

    async fn collect(&mut self) -> Vec<Event> {
        drop(self.trigger.take());
        let rx = match self.rx.take() {
            Some(rx) => rx,
            None => return Vec::new(),
        };
        let bodies: Vec<Bytes> = rx
            .collect::<Vec<_>>().await
            .into_iter().map(|(_, b)| b).collect();
        decode_bodies(bodies, &self.decompression, &self.decoding)
    }
}
```

## SocketGenerator

Wraps `send_lines()` from `src/test_util/mod.rs:137`.

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
            .map(|e| match e {
                Event::Log(log) => serde_json::to_string(log)
                    .map_err(|err| format!("failed to serialize event: {}", err)),
                _ => Err("socket generator only supports log events".to_string()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        send_lines(self.address, lines).await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
```

## TcpListener

Wraps `CountReceiver::receive_lines()` from `src/test_util/mod.rs:641`.

```rust
pub struct TcpListener {
    addr: SocketAddr,
    receiver: Option<CountReceiver<String>>,
}

#[async_trait]
impl TestListener for TcpListener {
    async fn start(&mut self) -> Result<(), String> {
        self.receiver = Some(CountReceiver::receive_lines(self.addr));
        wait_for_tcp_timeout(self.addr, Duration::from_secs(5)).await
            .map_err(|e| format!("listener '{}' failed to bind: {}", self.addr, e))
    }

    async fn collect(&mut self) -> Vec<Event> {
        match self.receiver.take() {
            Some(receiver) => receiver.await
                .into_iter()
                .map(|line| Event::Log(LogEvent::from_str_legacy(line)))
                .collect(),
            None => Vec::new(),
        }
    }
}
```

## Feature gate

`src/test_util/pipeline_test/` is gated behind the `test-utils` feature (the same feature that
gates most of `src/test_util/`). `build_pipeline_tests()` must be callable from `vector test`,
which is a production binary command, so `test-utils` is also enabled in the default feature
set for development builds. The `#[cfg(test)]` attribute is not used for this module.
