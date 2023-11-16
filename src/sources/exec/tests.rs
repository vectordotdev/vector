use crate::sources::exec::*;
use crate::{event::LogEvent, test_util::trace_init};
use bytes::Bytes;
use std::ffi::OsStr;
use std::io::Cursor;
use vector_lib::event::EventMetadata;
use vrl::value;

#[cfg(unix)]
use futures::task::Poll;

#[test]
fn test_generate_config() {
    crate::test_util::test_generate_config::<ExecConfig>();
}

#[test]
fn test_scheduled_handle_event() {
    let config = standard_scheduled_test_config();
    let hostname = Some("Some.Machine".to_string());
    let data_stream = Some(STDOUT.to_string());
    let pid = Some(8888_u32);

    let mut event = LogEvent::from("hello world").into();
    handle_event(
        &config,
        &hostname,
        &data_stream,
        pid,
        &mut event,
        LogNamespace::Legacy,
    );
    let log = event.as_log();

    assert_eq!(*log.get_host().unwrap(), "Some.Machine".into());
    assert_eq!(log[STREAM_KEY], STDOUT.into());
    assert_eq!(log[PID_KEY], (8888_i64).into());
    assert_eq!(log[COMMAND_KEY], config.command.into());
    assert_eq!(*log.get_message().unwrap(), "hello world".into());
    assert_eq!(*log.get_source_type().unwrap(), "exec".into());
    assert!(log.get_timestamp().is_some());
}

#[test]
fn test_scheduled_handle_event_vector_namespace() {
    let config = standard_scheduled_test_config();
    let hostname = Some("Some.Machine".to_string());
    let data_stream = Some(STDOUT.to_string());
    let pid = Some(8888_u32);

    let mut event: Event =
        LogEvent::from_parts(value!("hello world"), EventMetadata::default()).into();

    handle_event(
        &config,
        &hostname,
        &data_stream,
        pid,
        &mut event,
        LogNamespace::Vector,
    );

    let log = event.as_log();
    let meta = log.metadata().value();

    assert_eq!(
        meta.get(path!(ExecConfig::NAME, "host")).unwrap(),
        &value!("Some.Machine")
    );
    assert_eq!(
        meta.get(path!(ExecConfig::NAME, STREAM_KEY)).unwrap(),
        &value!(STDOUT)
    );
    assert_eq!(
        meta.get(path!(ExecConfig::NAME, PID_KEY)).unwrap(),
        &value!(8888_i64)
    );
    assert_eq!(
        meta.get(path!(ExecConfig::NAME, COMMAND_KEY)).unwrap(),
        &value!(config.command)
    );
    assert_eq!(log.value(), &value!("hello world"));
    assert_eq!(
        meta.get(path!("vector", "source_type")).unwrap(),
        &value!("exec")
    );
    assert!(meta
        .get(path!("vector", "ingest_timestamp"))
        .unwrap()
        .is_timestamp());
}

#[test]
fn test_streaming_create_event() {
    let config = standard_streaming_test_config();
    let hostname = Some("Some.Machine".to_string());
    let data_stream = Some(STDOUT.to_string());
    let pid = Some(8888_u32);

    let mut event = LogEvent::from("hello world").into();
    handle_event(
        &config,
        &hostname,
        &data_stream,
        pid,
        &mut event,
        LogNamespace::Legacy,
    );
    let log = event.as_log();

    assert_eq!(*log.get_host().unwrap(), "Some.Machine".into());
    assert_eq!(log[STREAM_KEY], STDOUT.into());
    assert_eq!(log[PID_KEY], (8888_i64).into());
    assert_eq!(log[COMMAND_KEY], config.command.into());
    assert_eq!(*log.get_message().unwrap(), "hello world".into());
    assert_eq!(*log.get_source_type().unwrap(), "exec".into());
    assert!(log.get_timestamp().is_some());
}

#[test]
fn test_streaming_create_event_vector_namespace() {
    let config = standard_streaming_test_config();
    let hostname = Some("Some.Machine".to_string());
    let data_stream = Some(STDOUT.to_string());
    let pid = Some(8888_u32);

    let mut event: Event =
        LogEvent::from_parts(value!("hello world"), EventMetadata::default()).into();

    handle_event(
        &config,
        &hostname,
        &data_stream,
        pid,
        &mut event,
        LogNamespace::Vector,
    );

    let log = event.as_log();
    let meta = event.metadata().value();

    assert_eq!(
        meta.get(path!(ExecConfig::NAME, "host")).unwrap(),
        &value!("Some.Machine")
    );
    assert_eq!(
        meta.get(path!(ExecConfig::NAME, STREAM_KEY)).unwrap(),
        &value!(STDOUT)
    );
    assert_eq!(
        meta.get(path!(ExecConfig::NAME, PID_KEY)).unwrap(),
        &value!(8888_i64)
    );
    assert_eq!(
        meta.get(path!(ExecConfig::NAME, COMMAND_KEY)).unwrap(),
        &value!(config.command)
    );
    assert_eq!(log.value(), &value!("hello world"));
    assert_eq!(
        meta.get(path!("vector", "source_type")).unwrap(),
        &value!("exec")
    );
    assert!(meta
        .get(path!("vector", "ingest_timestamp"))
        .unwrap()
        .is_timestamp());
}

#[test]
fn test_build_command() {
    let config = ExecConfig {
        mode: Mode::Streaming,
        scheduled: None,
        streaming: Some(StreamingConfig {
            respawn_on_exit: default_respawn_on_exit(),
            respawn_interval_secs: default_respawn_interval_secs(),
        }),
        command: vec!["./runner".to_owned(), "arg1".to_owned(), "arg2".to_owned()],
        environment: None,
        clear_environment: default_clear_environment(),
        working_directory: Some(PathBuf::from("/tmp")),
        include_stderr: default_include_stderr(),
        maximum_buffer_size_bytes: default_maximum_buffer_size(),
        framing: None,
        decoding: default_decoding(),
        log_namespace: None,
    };

    let command = build_command(&config);

    let mut expected_command = Command::new("./runner");
    expected_command.kill_on_drop(true);
    expected_command.current_dir("/tmp");
    expected_command.args(vec!["arg1".to_owned(), "arg2".to_owned()]);

    // Unfortunately the current_dir is not included in the formatted string
    let expected_command_string = format!("{:?}", expected_command);
    let command_string = format!("{:?}", command);

    assert_eq!(expected_command_string, command_string);
}

#[test]
fn test_build_command_custom_environment() {
    let config = ExecConfig {
        mode: Mode::Streaming,
        scheduled: None,
        streaming: Some(StreamingConfig {
            respawn_on_exit: default_respawn_on_exit(),
            respawn_interval_secs: default_respawn_interval_secs(),
        }),
        command: vec!["./runner".to_owned(), "arg1".to_owned(), "arg2".to_owned()],
        environment: Some(HashMap::from([("FOO".to_owned(), "foo".to_owned())])),
        clear_environment: default_clear_environment(),
        working_directory: Some(PathBuf::from("/tmp")),
        include_stderr: default_include_stderr(),
        maximum_buffer_size_bytes: default_maximum_buffer_size(),
        framing: None,
        decoding: default_decoding(),
        log_namespace: None,
    };

    let command = build_command(&config);
    let cmd = command.as_std();

    let idx = cmd
        .get_envs()
        .position(|v| v == (OsStr::new("FOO"), Some(OsStr::new("foo"))));

    assert_ne!(idx, None);
}

#[test]
fn test_build_command_clear_environment() {
    let config = ExecConfig {
        mode: Mode::Streaming,
        scheduled: None,
        streaming: Some(StreamingConfig {
            respawn_on_exit: default_respawn_on_exit(),
            respawn_interval_secs: default_respawn_interval_secs(),
        }),
        command: vec!["./runner".to_owned(), "arg1".to_owned(), "arg2".to_owned()],
        environment: Some(HashMap::from([("FOO".to_owned(), "foo".to_owned())])),
        clear_environment: true,
        working_directory: Some(PathBuf::from("/tmp")),
        include_stderr: default_include_stderr(),
        maximum_buffer_size_bytes: default_maximum_buffer_size(),
        framing: None,
        decoding: default_decoding(),
        log_namespace: None,
    };

    let command = build_command(&config);
    let cmd = command.as_std();

    let envs: Vec<_> = cmd.get_envs().collect();

    assert_eq!(envs.len(), 1);
}

#[tokio::test]
async fn test_spawn_reader_thread() {
    trace_init();

    let buf = Cursor::new("hello world\nhello rocket 🚀");
    let reader = BufReader::new(buf);
    let decoder = crate::codecs::Decoder::default();
    let (sender, mut receiver) = channel(1024);

    spawn_reader_thread(reader, decoder, STDOUT, sender);

    let mut counter = 0;
    if let Some(((events, byte_size), origin)) = receiver.recv().await {
        assert_eq!(byte_size, 11);
        assert_eq!(events.len(), 1);
        let log = events[0].as_log();
        assert_eq!(
            *log.get_message().unwrap(),
            Bytes::from("hello world").into()
        );
        assert_eq!(origin, STDOUT);
        counter += 1;
    }

    if let Some(((events, byte_size), origin)) = receiver.recv().await {
        assert_eq!(byte_size, 17);
        assert_eq!(events.len(), 1);
        let log = events[0].as_log();
        assert_eq!(
            *log.get_message().unwrap(),
            Bytes::from("hello rocket 🚀").into()
        );
        assert_eq!(origin, STDOUT);
        counter += 1;
    }

    assert_eq!(counter, 2);
}

#[tokio::test]
async fn test_drop_receiver() {
    let config = standard_scheduled_test_config();
    let hostname = Some("Some.Machine".to_string());
    let decoder = Default::default();
    let shutdown = ShutdownSignal::noop();
    let (tx, rx) = SourceSender::new_test();

    // Wait for our task to finish, wrapping it in a timeout
    let timeout = tokio::time::timeout(
        time::Duration::from_secs(5),
        run_command(
            config.clone(),
            hostname,
            decoder,
            shutdown,
            tx,
            LogNamespace::Legacy,
        ),
    );

    drop(rx);

    let _timeout_result = crate::test_util::components::assert_source_error(
        &crate::test_util::components::COMPONENT_ERROR_TAGS,
        timeout,
    )
    .await;
}

#[tokio::test]
#[cfg(unix)]
async fn test_run_command_linux() {
    let config = standard_scheduled_test_config();

    let (mut rx, timeout_result) = crate::test_util::components::assert_source_compliance(
        &crate::test_util::components::SOURCE_TAGS,
        async {
            let hostname = Some("Some.Machine".to_string());
            let decoder = Default::default();
            let shutdown = ShutdownSignal::noop();
            let (tx, rx) = SourceSender::new_test();

            // Wait for our task to finish, wrapping it in a timeout
            let result = tokio::time::timeout(
                time::Duration::from_secs(5),
                run_command(
                    config.clone(),
                    hostname,
                    decoder,
                    shutdown,
                    tx,
                    LogNamespace::Legacy,
                ),
            )
            .await;
            (rx, result)
        },
    )
    .await;

    let exit_status = timeout_result
        .expect("command timed out")
        .expect("command error");
    assert_eq!(0_i32, exit_status.unwrap().code().unwrap());

    if let Poll::Ready(Some(event)) = futures::poll!(rx.next()) {
        let log = event.as_log();
        assert_eq!(log[COMMAND_KEY], config.command.clone().into());
        assert_eq!(log[STREAM_KEY], STDOUT.into());
        assert_eq!(*log.get_source_type().unwrap(), "exec".into());
        assert_eq!(*log.get_message().unwrap(), "Hello World!".into());
        assert_eq!(*log.get_host().unwrap(), "Some.Machine".into());
        assert!(log.get(PID_KEY).is_some());
        assert!(log.get_timestamp().is_some());

        assert_eq!(8, log.all_event_fields().unwrap().count());
    } else {
        panic!("Expected to receive a linux event");
    }
}

#[tokio::test]
#[cfg(unix)]
async fn test_graceful_shutdown() {
    trace_init();
    let mut config = standard_streaming_test_config();
    config.command = vec![
        String::from("bash"),
        String::from("-c"),
        String::from(
            r#"trap 'echo signal received ; sleep 1; echo slept ; exit' SIGTERM; while true ; do sleep 10 ; done"#,
        ),
    ];
    let hostname = Some("Some.Machine".to_string());
    let decoder = Default::default();
    let (trigger, shutdown, _) = ShutdownSignal::new_wired();
    let (tx, mut rx) = SourceSender::new_test();

    let task = tokio::spawn(run_command(
        config.clone(),
        hostname,
        decoder,
        shutdown,
        tx,
        LogNamespace::Legacy,
    ));

    tokio::time::sleep(Duration::from_secs(1)).await; // let the source start the command

    drop(trigger); // start shutdown

    let exit_status = tokio::time::timeout(time::Duration::from_secs(30), task)
        .await
        .expect("join failed")
        .expect("command timed out")
        .expect("command error");

    assert_eq!(
        0_i32,
        exit_status.expect("missing exit status").code().unwrap()
    );

    if let Poll::Ready(Some(event)) = futures::poll!(rx.next()) {
        let log = event.as_log();
        assert_eq!(*log.get_message().unwrap(), "signal received".into());
    } else {
        panic!("Expected to receive event");
    }

    if let Poll::Ready(Some(event)) = futures::poll!(rx.next()) {
        let log = event.as_log();
        assert_eq!(*log.get_message().unwrap(), "slept".into());
    } else {
        panic!("Expected to receive event");
    }
}

fn standard_scheduled_test_config() -> ExecConfig {
    Default::default()
}

fn standard_streaming_test_config() -> ExecConfig {
    ExecConfig {
        mode: Mode::Streaming,
        scheduled: None,
        streaming: Some(StreamingConfig {
            respawn_on_exit: default_respawn_on_exit(),
            respawn_interval_secs: default_respawn_interval_secs(),
        }),
        command: vec!["yes".to_owned()],
        environment: None,
        clear_environment: default_clear_environment(),
        working_directory: None,
        include_stderr: default_include_stderr(),
        maximum_buffer_size_bytes: default_maximum_buffer_size(),
        framing: None,
        decoding: default_decoding(),
        log_namespace: None,
    }
}
