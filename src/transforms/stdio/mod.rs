mod config;
mod exec;
mod process;
mod session;
mod transform;

#[cfg(test)]
mod tests {
    #![allow(clippy::print_stderr)]

    use std::{collections::HashMap, io, pin::Pin, time::Duration};

    use futures::{Stream, StreamExt as _};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        sync::mpsc::{self, Receiver, Sender},
        task::JoinSet,
        time::{Instant, sleep, timeout},
    };
    use tokio_stream::wrappers::ReceiverStream;
    use vector_lib::{
        codecs::{
            JsonDeserializerConfig, JsonSerializerConfig, NewlineDelimitedDecoderConfig,
            NewlineDelimitedEncoderConfig, encoding,
        },
        config::LogNamespace,
        transform::TaskTransform as _,
    };
    use vrl::core::Value;

    use crate::{
        codecs::{self, DecodingConfig},
        event::{Event, LogEvent},
        test_util::components::assert_transform_compliance,
        transforms::{
            stdio::{
                config::{
                    CommandConfig, Mode, PerEventConfig, ScheduledConfig, StdioConfig,
                    StreamingConfig,
                },
                exec::ExecTask,
                process::mock::MockSpawnerBuilder,
            },
            test::create_topology,
        },
    };

    #[tokio::test]
    async fn test_integration() {
        struct TestCase {
            mode: Mode,
            command: Vec<&'static str>,
            tx: Vec<&'static str>,
            rx: Vec<&'static str>,
            scheduled: Option<ScheduledConfig>,
            per_event: Option<PerEventConfig>,
            vars: HashMap<&'static str, &'static str>,
            clear_vars: bool,
        }

        impl Default for TestCase {
            fn default() -> Self {
                Self {
                    mode: Mode::PerEvent,
                    command: vec!["cat"],
                    tx: vec![],
                    rx: vec![],
                    scheduled: None,
                    per_event: None,
                    vars: HashMap::new(),
                    clear_vars: false,
                }
            }
        }

        let cases = vec![
            (
                "per-event",
                TestCase {
                    mode: Mode::PerEvent,
                    per_event: Some(PerEventConfig {
                        // Guarantee ordering of events.
                        max_concurrent_processes: 1,
                    }),
                    command: vec!["cat"],
                    tx: vec!["hello", "world"],
                    rx: vec![r#"{"message":"hello"}"#, r#"{"message":"world"}"#],
                    ..Default::default()
                },
            ),
            (
                "streaming",
                TestCase {
                    mode: Mode::Streaming,
                    command: vec!["cat"],
                    tx: vec!["hello", "world"],
                    rx: vec![r#"{"message":"hello"}"#, r#"{"message":"world"}"#],
                    ..Default::default()
                },
            ),
            (
                "scheduled",
                TestCase {
                    mode: Mode::Scheduled,
                    command: vec!["cat"],
                    tx: vec!["hello", "world"],
                    rx: vec![r#"{"message":"hello"}"#, r#"{"message":"world"}"#],
                    scheduled: Some(ScheduledConfig {
                        exec_interval_secs: 0.1,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ),
            (
                "environment variables",
                TestCase {
                    mode: Mode::PerEvent,
                    command: vec!["echo $TEST_VAR"],
                    tx: vec!["trigger"],
                    rx: vec!["test_value"],
                    vars: HashMap::from([("TEST_VAR", "test_value")]),
                    ..Default::default()
                },
            ),
            (
                "clear environment variables",
                TestCase {
                    mode: Mode::PerEvent,
                    command: vec![r#"echo "PATH=$PATH""#],
                    tx: vec!["trigger"],
                    rx: vec!["PATH="],
                    clear_vars: true,
                    ..Default::default()
                },
            ),
        ];

        for (
            name,
            TestCase {
                mode,
                command,
                tx,
                rx,
                scheduled,
                per_event,
                vars,
                clear_vars,
            },
        ) in cases
        {
            eprintln!("running test case {}", name);

            if command.is_empty() || which::which(command[0]).is_err() {
                eprintln!(
                    "Skipping integration test '{}' because command {:?} is not available",
                    name, command
                );

                continue;
            }

            assert_transform_compliance(async {
                let config = StdioConfig {
                    mode,
                    command: CommandConfig {
                        command: command.into_iter().map(str::to_owned).collect(),
                        environment: (!vars.is_empty()).then_some(
                            vars.into_iter()
                                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                                .collect(),
                        ),
                        clear_environment: clear_vars,
                        ..Default::default()
                    },
                    scheduled,
                    per_event,
                    ..Default::default()
                };

                let (stream_tx, stream_rx) = mpsc::channel(1);
                let (topology, mut out) =
                    create_topology(ReceiverStream::new(stream_rx), config).await;

                for event in tx {
                    send_event(&stream_tx, event).await;
                }
                for event in rx {
                    assert_eq!(
                        &Value::from(event),
                        recv_event(&mut out)
                            .await
                            .expect("Expected an event but got None")
                            .as_log()
                            .get("message")
                            .unwrap()
                    );
                }

                drop(stream_tx);
                topology.stop().await;

                assert_eq!(out.recv().await, None);
            })
            .await;
        }
    }

    #[tokio::test]
    async fn test_stdout_success() {
        for mode in [Mode::PerEvent, Mode::Streaming, Mode::Scheduled] {
            let case = TestCase {
                mode,
                capture_stderr: false,
                scheduled: Some(ScheduledConfig {
                    exec_interval_secs: 0.1,
                    buffer_size: 10,
                }),
                streaming: Some(StreamingConfig {
                    // Ensure Streaming mode exits when input closes and child
                    // exits.
                    respawn_on_exit: false,
                    respawn_interval_secs: 0,
                }),
                per_event: None,
                stderr_decoder: None,
                spawner: |builder: &mut MockSpawnerBuilder, joins: &mut JoinSet<()>| {
                    let mut handle = builder.expect_spawn(false);

                    joins.spawn(async move {
                        let mut buf = vec![0u8; 4096];
                        while let Ok(n) = handle.stdin.read(&mut buf).await {
                            if n == 0 {
                                break;
                            }
                            let _ = handle.stdout.write_all(&buf[..n]).await;
                        }

                        handle.exit(0);
                    });
                },
                runner: async |tx: Sender<Event>, rx: Pin<Box<dyn Stream<Item = Event>>>| {
                    let msg = "test_message";
                    send_event(&tx, msg).await;
                    drop(tx);

                    let events: Vec<_> = rx.collect().await;

                    assert_eq!(events.len(), 1);
                    assert_eq!(
                        events[0].as_log().get("message").unwrap(),
                        &Value::from(msg)
                    );
                },
            };

            case.run().await;
        }
    }

    #[tokio::test]
    async fn test_streaming_respawn() {
        let case = TestCase {
            mode: Mode::Streaming,
            capture_stderr: false,
            scheduled: None,
            streaming: Some(StreamingConfig {
                respawn_on_exit: true,
                respawn_interval_secs: 0,
            }),
            per_event: None,
            stderr_decoder: None,
            spawner: |builder: &mut MockSpawnerBuilder, joins: &mut JoinSet<()>| {
                // First process exits immediately
                let handle1 = builder.expect_spawn(false);
                joins.spawn(async move {
                    handle1.exit(1);
                });

                // Second process after respawn prints to stdout
                let mut handle2 = builder.expect_spawn(false);
                joins.spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    if let Ok(n) = handle2.stdin.read(&mut buf).await {
                        let _ = handle2.stdout.write_all(&buf[..n]).await;
                    }
                    handle2.exit(0);
                });
            },
            runner: async |tx: Sender<Event>, mut rx: Pin<Box<dyn Stream<Item = Event>>>| {
                // Send event (should go to second process)
                tx.send(Event::from(LogEvent::from("after_respawn")))
                    .await
                    .unwrap();

                let event = timeout(Duration::from_secs(1), rx.next())
                    .await
                    .unwrap()
                    .unwrap();

                assert_eq!(
                    event.as_log().get("message").unwrap(),
                    &Value::from("after_respawn")
                );
            },
        };

        case.run().await;
    }

    #[tokio::test]
    async fn test_scheduled_buffer_overflow() {
        let case = TestCase {
            mode: Mode::Scheduled,
            capture_stderr: false,
            scheduled: Some(ScheduledConfig {
                // Set a large interval so the timer never fires during the test.
                // We rely entirely on the buffer overflow logic happening
                // during ingestion, and the "flush on shutdown" to emit the
                // remaining events.
                exec_interval_secs: 10.0,
                buffer_size: 2,
            }),
            streaming: None,
            per_event: None,
            stderr_decoder: None,
            spawner: |builder: &mut MockSpawnerBuilder, joins: &mut JoinSet<()>| {
                // We expect ONE spawn that receives the final batch of 2.
                let mut handle = builder.expect_spawn(false);
                joins.spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    while let Ok(n) = handle.stdin.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                        handle.stdout.write_all(&buf[..n]).await.unwrap();
                    }

                    handle.exit(0);
                });
            },
            runner: async |tx: Sender<Event>, rx: Pin<Box<dyn Stream<Item = Event>>>| {
                // Send 5 events, retaining the last 2.
                for i in 1..=5 {
                    let mut log = LogEvent::default();
                    log.insert("counter", i);
                    tx.send(Event::from(log)).await.unwrap();
                }

                // Close input stream to trigger the flush of [4, 5].
                drop(tx);

                let events: Vec<_> = rx.collect().await;
                assert_eq!(events.len(), 2,);
                assert_eq!(events[0].as_log().get("counter").unwrap(), &Value::from(4));
                assert_eq!(events[1].as_log().get("counter").unwrap(), &Value::from(5));
            },
        };

        case.run().await;
    }

    #[tokio::test]
    async fn test_scheduled_process_timeout() {
        let exec_interval_secs = 0.2;

        let case = TestCase {
            mode: Mode::Scheduled,
            capture_stderr: false,
            scheduled: Some(ScheduledConfig {
                exec_interval_secs,
                buffer_size: 10,
            }),
            streaming: None,
            per_event: None,
            stderr_decoder: None,
            spawner: |builder: &mut MockSpawnerBuilder, joins: &mut JoinSet<()>| {
                let handle1 = builder.expect_spawn(false);
                joins.spawn(async move {
                    // Keep the handle alive longer than `exec_interval_secs` to
                    // ensure the component triggers the timeout logic before
                    // this task exits naturally.
                    sleep(Duration::from_secs_f64(exec_interval_secs + 0.1)).await;
                    drop(handle1);
                });

                // A new process is spawned after the first one times out.
                let mut handle2 = builder.expect_spawn(false);
                joins.spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    while let Ok(n) = handle2.stdin.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                        let _ = handle2.stdout.write_all(&buf[..n]).await;
                    }
                    handle2.exit(0);
                });
            },
            runner: async |tx: Sender<Event>, mut rx: Pin<Box<dyn Stream<Item = Event>>>| {
                // First batch is sent to the first process, which hangs.
                send_event(&tx, "batch_1").await;

                // Wait for the first process to timeout. During this period, no
                // events should be received.
                let result =
                    timeout(Duration::from_secs_f64(exec_interval_secs + 0.1), rx.next()).await;
                assert!(result.is_err());

                // Second batch is processed as expected by the second process.
                send_event(&tx, "batch_2").await;

                let event = timeout(Duration::from_secs(1), rx.next())
                    .await
                    .expect("Should receive output from Batch 2")
                    .expect("Stream should not be closed");

                assert_eq!(
                    event.as_log().get("message").unwrap(),
                    &Value::from("batch_2")
                );
            },
        };

        case.run().await;
    }

    #[tokio::test]
    async fn test_streaming_fatal_error_no_retry() {
        let case = TestCase {
            mode: Mode::Streaming,
            capture_stderr: false,
            scheduled: None,
            streaming: Some(StreamingConfig {
                respawn_on_exit: true,
                respawn_interval_secs: 0,
            }),
            per_event: None,
            stderr_decoder: None,
            spawner: |builder: &mut MockSpawnerBuilder, _| {
                // Even though `respawn_on_exit` is true, fatal errors stop the
                // loop.
                builder.expect_spawn_error(io::Error::new(io::ErrorKind::NotFound, "not found"));

                // NO new processes are expected to spawn after the first one
                // errors.
            },
            runner: async |_, mut rx: Pin<Box<dyn Stream<Item = Event>>>| {
                // The stream should close immediately upon encountering the
                // fatal error
                match timeout(Duration::from_secs(1), rx.next()).await {
                    Ok(None) => {} // Pass: Stream closed
                    Ok(Some(_)) => panic!("Should not receive events"),
                    Err(_) => panic!("Stream hung (likely retrying infinitely)"),
                }
            },
        };

        case.run().await;
    }

    #[tokio::test]
    async fn test_scheduled_flush_on_shutdown() {
        let case = TestCase {
            mode: Mode::Scheduled,
            capture_stderr: false,
            // Set a long interval to ensure the ticker doesn't fire naturally.
            scheduled: Some(ScheduledConfig {
                exec_interval_secs: 10.0,
                buffer_size: 10,
            }),
            streaming: None,
            per_event: None,
            stderr_decoder: None,
            spawner: |builder: &mut MockSpawnerBuilder, joins: &mut JoinSet<()>| {
                // We expect a spawn because the shutdown should flush the
                // buffer.
                let mut handle = builder.expect_spawn(false);
                joins.spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    if let Ok(n) = handle.stdin.read(&mut buf).await {
                        let _ = handle.stdout.write_all(&buf[..n]).await;
                    }
                    handle.exit(0);
                });
            },
            runner: async |tx: Sender<Event>, mut rx: Pin<Box<dyn Stream<Item = Event>>>| {
                send_event(&tx, "buffered_event").await;

                // Close the input stream immediately.
                drop(tx);

                // We expect the component to flush the buffer before shutting
                // down.
                let event = timeout(Duration::from_secs(1), rx.next())
                    .await
                    .expect("Should not timeout waiting for flush")
                    .expect("Stream should contain flushed event");

                assert_eq!(
                    event.as_log().get("message").unwrap(),
                    &Value::from("buffered_event")
                );
            },
        };

        case.run().await;
    }

    #[tokio::test]
    async fn test_per_event_concurrency() {
        let case = TestCase {
            mode: Mode::PerEvent,
            capture_stderr: false,
            scheduled: None,
            streaming: None,
            per_event: None,
            stderr_decoder: None,
            spawner: |builder: &mut MockSpawnerBuilder, joins: &mut JoinSet<()>| {
                // Expect 5 concurrent spawns
                for _ in 0..5 {
                    let mut handle = builder.expect_spawn(false);
                    joins.spawn(async move {
                        // Wait for the process to actually read from stdin
                        // (triggering the start of the sleep).
                        let mut buf = [0u8; 1024];
                        let _ = handle.stdin.read(&mut buf).await;

                        // Simulate a slow process.
                        sleep(Duration::from_millis(200)).await;
                        // Write output so we get an event back
                        let _ = handle.stdout.write_all(b"{\"message\":\"done\"}\n").await;
                        handle.exit(0);
                    });
                }
            },
            runner: async |tx, mut rx| {
                let start = Instant::now();

                // Send 5 events rapidly
                for i in 0..5 {
                    send_event(&tx, &format!("trigger_{}", i)).await;
                }

                // Drop tx to close input stream.
                drop(tx);

                // Collect all 5 results.
                for _ in 0..5 {
                    let _ = timeout(Duration::from_secs(1), rx.next()).await.unwrap();
                }

                let elapsed = start.elapsed();

                // If sequential: 5 * 200ms = 1000ms. If concurrent: ~200ms + overhead.
                assert!(
                    elapsed < Duration::from_millis(500),
                    "Processes ran sequentially (took {:?})",
                    elapsed
                );
            },
        };
        case.run().await;
    }

    #[tokio::test]
    async fn test_per_event_concurrency_limit() {
        let case = TestCase {
            mode: Mode::PerEvent,
            capture_stderr: false,
            scheduled: None,
            streaming: None,
            per_event: Some(PerEventConfig {
                max_concurrent_processes: 2,
            }),
            stderr_decoder: None,
            spawner: |builder: &mut MockSpawnerBuilder, joins: &mut JoinSet<()>| {
                // Expect 5 tasks
                for _ in 0..5 {
                    let mut handle = builder.expect_spawn(false);
                    joins.spawn(async move {
                        // Wait for the process to actually read from stdin
                        // (triggering the start of the sleep).
                        let mut buf = [0u8; 1024];
                        let _ = handle.stdin.read(&mut buf).await;

                        sleep(Duration::from_millis(100)).await;
                        let _ = handle.stdout.write_all(b"{\"message\":\"done\"}\n").await;
                        handle.exit(0);
                    });
                }
            },
            runner: async |tx: Sender<Event>, rx: Pin<Box<dyn Stream<Item = Event>>>| {
                let start = Instant::now();

                for i in 0..5 {
                    send_event(&tx, &format!("trigger_{}", i)).await;
                }
                drop(tx);

                let events: Vec<_> = rx.collect().await;
                assert_eq!(events.len(), 5, "Expected 5 successful events");

                let elapsed = start.elapsed();

                // 5 tasks, each taking ~100ms, two tasks running concurrently.
                // The total time should be ~300ms.

                assert!(
                    elapsed > Duration::from_millis(200),
                    "Processes ran too fast (concurrency limit likely ignored). Took: {:?}",
                    elapsed
                );

                assert!(
                    elapsed < Duration::from_millis(400),
                    "Processes ran sequentially. Took: {:?}",
                    elapsed
                );
            },
        };

        case.run().await;
    }

    #[tokio::test]
    async fn test_decode_error() {
        for mode in [Mode::PerEvent, Mode::Streaming, Mode::Scheduled] {
            let case = TestCase {
                mode,
                capture_stderr: false,
                scheduled: Some(ScheduledConfig {
                    exec_interval_secs: 0.1,
                    buffer_size: 2,
                }),
                streaming: Some(StreamingConfig {
                    // We want the original process to not exit.
                    respawn_on_exit: false,
                    respawn_interval_secs: 0,
                }),
                per_event: None,
                stderr_decoder: None,
                spawner: |builder: &mut MockSpawnerBuilder, joins: &mut JoinSet<()>| {
                    // Only one process is spawned, it must not fail.
                    let mut handle = builder.expect_spawn(false);

                    joins.spawn(async move {
                        let mut buf = vec![0u8; 100];
                        let _ = handle.stdin.read(&mut buf).await;

                        // Second event is invalid JSON.
                        let _ = handle.stdout.write_all(b"{\"message\":\"good1\"}\n").await;
                        let _ = handle.stdout.write_all(b"not json\n").await;
                        let _ = handle.stdout.write_all(b"{\"message\":\"good2\"}\n").await;

                        handle.exit(0);
                    });
                },
                runner: async |tx: Sender<Event>, mut rx: Pin<Box<dyn Stream<Item = Event>>>| {
                    // Trigger the process.
                    send_event(&tx, "start").await;

                    // Expect first valid event.
                    let e1 = timeout(Duration::from_secs(1), rx.next())
                        .await
                        .expect("Timeout waiting for 1st event")
                        .expect("Stream closed unexpectedly");

                    assert_eq!(e1.as_log().get("message").unwrap(), &Value::from("good1"));

                    // Expect second valid event (skipping the invalid one).
                    let e2 = timeout(Duration::from_secs(1), rx.next())
                        .await
                        .expect("Timeout waiting for 2nd event (decode error likely killed stream)")
                        .expect("Stream closed unexpectedly");

                    assert_eq!(e2.as_log().get("message").unwrap(), &Value::from("good2"));
                },
            };

            case.run().await;
        }
    }

    #[tokio::test]
    async fn test_stderr_handling() {
        for mode in [Mode::PerEvent, Mode::Streaming, Mode::Scheduled] {
            for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
                let case = TestCase {
                    mode,
                    capture_stderr: true,
                    scheduled: Some(ScheduledConfig {
                        exec_interval_secs: 0.1,
                        buffer_size: 2,
                    }),
                    streaming: Some(StreamingConfig {
                        respawn_on_exit: false,
                        respawn_interval_secs: 0,
                    }),
                    per_event: None,
                    stderr_decoder: Some(codecs::DecodingConfig::new(
                        NewlineDelimitedDecoderConfig::default().into(),
                        JsonDeserializerConfig::default().into(),
                        namespace,
                    )),
                    spawner: |builder: &mut MockSpawnerBuilder, joins: &mut JoinSet<()>| {
                        let mut handle = builder.expect_spawn(true);
                        joins.spawn(async move {
                            let mut buf = vec![0u8; 4096];

                            if let Ok(n) = handle.stdin.read(&mut buf).await {
                                let _ = handle.stdout.write_all(&buf[..n]).await;

                                // Also write to stderr
                                if let Some(ref mut stderr) = handle.stderr {
                                    let stderr_msg = b"{\"message\":\"stderr_output\"}\n";
                                    let _ = stderr.write_all(stderr_msg).await;
                                }
                            }
                            handle.exit(0);
                        });
                    },
                    runner:
                        async |tx: Sender<Event>, mut rx: Pin<Box<dyn Stream<Item = Event>>>| {
                            // Send an event
                            send_event(&tx, "test_input").await;

                            // Should receive events from both stdout and stderr, in
                            // random order.
                            let mut received_stdout = false;
                            let mut received_stderr = false;

                            for _ in 0..2 {
                                if let Ok(Some(event)) =
                                    timeout(Duration::from_secs(1), rx.next()).await
                                {
                                    let log = event.as_log();

                                    // Check the stream tag.
                                    let tag = log
                                        .get("metadata.stream")
                                        .or_else(|| log.get("%vector.stream"))
                                        .expect("Missing stream tag");

                                    match tag.to_string_lossy().as_ref() {
                                        "stdout" => received_stdout = true,
                                        "stderr" => received_stderr = true,
                                        _ => {}
                                    }
                                }
                            }

                            assert!(received_stdout, "Mode {:?} missing stdout event", mode);
                            assert!(received_stderr, "Mode {:?} missing stderr event", mode);
                        },
                };

                case.run().await;
            }
        }
    }

    #[tokio::test]
    async fn test_stderr_ignored() {
        for mode in [Mode::PerEvent, Mode::Streaming, Mode::Scheduled] {
            let case = TestCase {
                mode,
                capture_stderr: false,
                scheduled: Some(ScheduledConfig {
                    exec_interval_secs: 0.1,
                    buffer_size: 2,
                }),
                streaming: Some(StreamingConfig {
                    respawn_on_exit: false,
                    respawn_interval_secs: 0,
                }),
                per_event: None,
                stderr_decoder: None,
                spawner: |builder: &mut MockSpawnerBuilder, joins: &mut JoinSet<()>| {
                    let mut handle = builder.expect_spawn(true);
                    joins.spawn(async move {
                        let mut buf = vec![0u8; 100];
                        let _ = handle.stdin.read(&mut buf).await;

                        // Write to stdout
                        let _ = handle
                            .stdout
                            .write_all(b"{\"message\":\"from_stdout\"}\n")
                            .await;

                        // Write to stderr
                        //
                        // Fails with `BrokenPipe` because we're not capturing
                        // stderr.
                        if let Some(stderr) = handle.stderr.as_mut() {
                            let res = stderr.write_all(b"{\"message\":\"from_stderr\"}\n").await;
                            assert_eq!(res.unwrap_err().kind(), io::ErrorKind::BrokenPipe,);
                        }

                        handle.exit(0);
                    });
                },
                runner: async |tx: Sender<Event>, rx: Pin<Box<dyn Stream<Item = Event>>>| {
                    send_event(&tx, "trigger").await;

                    // Close the input stream.
                    drop(tx);

                    let events: Vec<_> = rx.collect().await;
                    assert_eq!(events.len(), 1);
                    assert_eq!(
                        events[0].as_log().get("message").unwrap(),
                        &Value::from("from_stdout"),
                    );
                },
            };
            case.run().await;
        }
    }

    fn create_test_encoder() -> codecs::Encoder<encoding::Framer> {
        let serializer = JsonSerializerConfig::default().build().into();
        let framer = NewlineDelimitedEncoderConfig.build().into();

        codecs::Encoder::<encoding::Framer>::new(framer, serializer)
    }

    fn create_test_decoder() -> codecs::Decoder {
        codecs::DecodingConfig::new(
            NewlineDelimitedDecoderConfig::default().into(),
            JsonDeserializerConfig::default().into(),
            LogNamespace::Legacy,
        )
        .build()
        .unwrap()
    }

    async fn send_event(tx: &Sender<Event>, message: &str) {
        let mut log = LogEvent::default();
        log.insert("message", message);

        tx.send(Event::from(log)).await.unwrap();
    }

    async fn recv_event(out: &mut Receiver<Event>) -> Option<Event> {
        timeout(Duration::from_secs(1), out.recv())
            .await
            .ok()
            .flatten()
    }

    struct TestCase<
        S: Fn(&mut MockSpawnerBuilder, &mut JoinSet<()>),
        R: AsyncFnOnce(Sender<Event>, Pin<Box<dyn Stream<Item = Event>>>),
    > {
        mode: Mode,
        capture_stderr: bool,
        scheduled: Option<ScheduledConfig>,
        streaming: Option<StreamingConfig>,
        per_event: Option<PerEventConfig>,
        stderr_decoder: Option<DecodingConfig>,
        spawner: S,
        runner: R,
    }

    impl<S, R> TestCase<S, R>
    where
        S: Fn(&mut MockSpawnerBuilder, &mut JoinSet<()>),
        R: AsyncFnOnce(Sender<Event>, Pin<Box<dyn Stream<Item = Event>>>),
    {
        async fn run(self) {
            let Self {
                mode,
                capture_stderr,
                scheduled,
                streaming,
                per_event,
                stderr_decoder,
                spawner,
                runner,
            } = self;

            let mut mock = MockSpawnerBuilder::new();
            let mut joins = JoinSet::new();
            spawner(&mut mock, &mut joins);

            let task = ExecTask {
                command: CommandConfig::default(),
                mode,
                scheduled,
                streaming,
                per_event,
                capture_stderr,
                spawner: mock.build(),
                stdin_encoder: create_test_encoder(),
                stdout_decoder: create_test_decoder(),
                stderr_decoder: capture_stderr.then(|| {
                    stderr_decoder
                        .and_then(|v| v.build().ok())
                        .unwrap_or_else(create_test_decoder)
                }),
            };

            let (tx, rx) = mpsc::channel(10);
            let input = ReceiverStream::new(rx);

            let output = Box::new(task).transform(Box::pin(input));
            runner(tx, output).await;
            joins.join_all().await;
        }
    }
}
