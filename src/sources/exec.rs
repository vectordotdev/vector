use crate::config::{DataType, GlobalOptions};
use crate::internal_events::ExecTimeout;
use crate::{
    config::{log_schema, SourceConfig, SourceDescription},
    event::Event,
    internal_events::{ExecEventReceived, ExecFailed},
    shutdown::ShutdownSignal,
    Pipeline,
};
use bytes::Bytes;
use derive_is_enum_variant::is_enum_variant;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader, Lines};
use tokio::process::Command;
use tokio::sync::mpsc::{channel, Sender};
use tokio::time;

#[derive(Deserialize, Serialize, Debug, Clone)]
// TODO: add back when serde-rs/serde#1358 is addressed (same as syslog)
// #[serde(deny_unknown_fields)]
#[serde(default)]
pub struct ExecConfig {
    #[serde(flatten)]
    pub mode: Mode,
    pub command: String,
    pub arguments: Option<Vec<String>>,
    pub current_dir: Option<PathBuf>,
    pub include_stderr: Option<bool>,
    #[serde(default = "default_host_key")]
    pub host_key: String,
    pub data_stream_key: Option<String>,
    pub pid_key: Option<String>,
    pub exit_status_key: Option<String>,
    pub command_key: Option<String>,
    pub arguments_key: Option<String>,
    #[serde(skip, default = "default_hostname")]
    pub hostname: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, is_enum_variant)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Scheduled {
        #[serde(default = "default_exec_interval_secs")]
        exec_interval_secs: u64,
        #[serde(default = "default_events_per_line")]
        event_per_line: bool,
        #[serde(default = "default_exec_duration_millis_key")]
        exec_duration_millis_key: String,
    },
    Streaming {
        #[serde(default = "default_respawn_on_exit")]
        respawn_on_exit: bool,
        #[serde(default = "default_respawn_interval_secs")]
        respawn_interval_secs: u64,
    },
}

impl Default for ExecConfig {
    fn default() -> Self {
        ExecConfig {
            mode: Mode::Scheduled {
                exec_interval_secs: default_exec_interval_secs(),
                event_per_line: default_events_per_line(),
                exec_duration_millis_key: default_exec_duration_millis_key(),
            },
            command: "echo".to_owned(),
            arguments: Some(vec!["Hello World!".to_owned()]),
            current_dir: None,
            include_stderr: Some(true),
            host_key: default_host_key(),
            data_stream_key: Some("data_stream".to_string()),
            pid_key: Some("pid".to_string()),
            exit_status_key: Some("exit_status".to_string()),
            command_key: None,
            arguments_key: None,
            hostname: default_hostname(),
        }
    }
}

fn default_exec_interval_secs() -> u64 {
    60
}

fn default_respawn_interval_secs() -> u64 {
    60
}

fn default_respawn_on_exit() -> bool {
    true
}

fn default_host_key() -> String {
    log_schema().host_key().to_string()
}

fn default_events_per_line() -> bool {
    true
}

fn default_exec_duration_millis_key() -> String {
    "exec_duration_millis".to_string()
}

fn default_hostname() -> Option<String> {
    crate::get_hostname().ok()
}

pub const EXEC: &str = "exec";
pub const STDOUT: &str = "stdout";
pub const STDERR: &str = "stderr";

inventory::submit! {
    SourceDescription::new::<ExecConfig>("exec")
}

impl_generate_config_from_default!(ExecConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "exec")]
impl SourceConfig for ExecConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        match self.mode.clone() {
            Mode::Scheduled {
                exec_interval_secs,
                event_per_line,
                exec_duration_millis_key,
            } => run_scheduled(
                self.clone(),
                exec_interval_secs,
                event_per_line,
                exec_duration_millis_key,
                shutdown,
                out,
            ),
            Mode::Streaming {
                respawn_on_exit,
                respawn_interval_secs,
            } => run_streaming(
                self.clone(),
                respawn_on_exit,
                respawn_interval_secs,
                shutdown,
                out,
            ),
        }
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        EXEC
    }
}

pub fn run_scheduled(
    config: ExecConfig,
    exec_interval_secs: u64,
    event_per_line: bool,
    exec_duration_millis_key: String,
    shutdown: ShutdownSignal,
    mut out: Pipeline,
) -> crate::Result<super::Source> {
    Ok(Box::pin(async move {
        info!("Starting scheduled exec run.");
        let duration = time::Duration::from_secs(exec_interval_secs);
        let mut interval = time::interval(duration).take_until(shutdown);

        while interval.next().await.is_some() {
            let output = run_once_scheduled(
                &config,
                exec_interval_secs,
                event_per_line,
                exec_duration_millis_key.clone(),
            )
            .await;

            if let Some(events) = output {
                for event in events {
                    out.send(event)
                        .await
                        .map_err(|_: crate::pipeline::ClosedError| {
                            error!(message = "Failed to forward events; downstream is closed.");
                        })?;
                }
            }
        }

        info!("Finished scheduled exec run.");
        Ok(())
    }))
}

pub fn run_streaming(
    config: ExecConfig,
    respawn_on_exit: bool,
    respawn_interval_secs: u64,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> crate::Result<super::Source> {
    Ok(Box::pin(async move {
        if respawn_on_exit {
            let duration = time::Duration::from_secs(respawn_interval_secs);

            // Continue to loop while shutdown is pending
            while futures::poll!(shutdown.clone()).is_pending() {
                let _ = run_once_streaming(config.clone(), shutdown.clone(), out.clone()).await;

                if futures::poll!(shutdown.clone()).is_ready() {
                    break;
                } else {
                    warn!("Streaming processed ended before shutdown.");

                    // Using time interval so it can utilize the shutdown signal
                    let mut interval = time::interval(duration).take_until(shutdown.clone());

                    // Call next twice since the first is immediate
                    interval.next().await;
                    interval.next().await;
                }
            }
        } else {
            let _ = run_once_streaming(config, shutdown, out).await;
        }

        Ok(())
    }))
}

async fn run_once_streaming(
    config: ExecConfig,
    shutdown: ShutdownSignal,
    mut out: Pipeline,
) -> Result<(), ()> {
    info!("Starting streaming exec run.");
    let mut command = build_command(&config);
    let child = command.spawn();
    match child {
        Ok(mut child) => {
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();

            // Create stdout async reader
            let stdout_reader = BufReader::new(stdout);
            let stdout_lines = stdout_reader.lines();

            // Create stderr async reader
            let stderr_reader = BufReader::new(stderr);
            let stderr_lines = stderr_reader.lines();

            // Set up communication channels
            let (sender, mut receiver) = channel(1024);
            let error_sender = sender.clone();

            let stdout_shutdown = shutdown.clone();
            let stderr_shutdown = shutdown.clone();

            let pid = child.id();

            spawn_streaming_thread(stdout_lines, stdout_shutdown, STDOUT, sender);
            spawn_streaming_thread(stderr_lines, stderr_shutdown, STDERR, error_sender);

            while let Some((line, stream)) = receiver.recv().await {
                let event = create_event(
                    &config,
                    Bytes::from(line),
                    &None,
                    &Some(stream.to_string()),
                    Some(pid),
                    None,
                    &None,
                );

                out.send(event)
                    .await
                    .map_err(|_: crate::pipeline::ClosedError| {
                        error!(message = "Failed to forward events; downstream is closed.");
                    })?;
            }

            info!("Finished streaming exec run.");
            let _ = out.flush().await;
            Ok(())
        }
        Err(spawn_error) => {
            error!("Error during streaming exec run.");
            emit!(ExecFailed { error: spawn_error });

            Err(())
        }
    }
}

async fn run_once_scheduled(
    config: &ExecConfig,
    exec_interval_secs: u64,
    event_per_line: bool,
    exec_duration_millis_key: String,
) -> Option<Vec<Event>> {
    let mut command = build_command(config);

    // Mark the start time just before spawning the process as
    // this seems to be the best approximation of exec duration
    let now = Instant::now();
    let child = command.spawn();

    match child {
        Ok(child) => {
            let schedule = Duration::from_secs(exec_interval_secs);
            let pid = child.id();

            // Wait for our task to finish, wrapping it in a timeout
            let timeout = tokio::time::timeout(schedule, child.wait_with_output());
            let timeout_result = timeout.await;

            match timeout_result {
                Ok(output) => match output {
                    Ok(output) => {
                        let mut events = Vec::new();
                        let elapsed_millis = now.elapsed().as_millis();
                        let exit_status = output.status.code();

                        let stdout_events = process_scheduled_output(
                            config,
                            event_per_line,
                            exec_duration_millis_key.clone(),
                            pid,
                            exit_status,
                            elapsed_millis,
                            output.stdout,
                            STDOUT,
                        );

                        if let Some(mut stdout_events) = stdout_events {
                            events.append(&mut stdout_events);
                        }

                        if config.include_stderr.unwrap_or(true) {
                            let stderr_events = process_scheduled_output(
                                config,
                                event_per_line,
                                exec_duration_millis_key.clone(),
                                pid,
                                exit_status,
                                elapsed_millis,
                                output.stderr,
                                STDERR,
                            );

                            if let Some(mut stderr_events) = stderr_events {
                                events.append(&mut stderr_events);
                            }
                        }

                        if events.is_empty() {
                            None
                        } else {
                            Some(events)
                        }
                    }
                    Err(command_error) => {
                        error!("Error during scheduled exec run.");
                        emit!(ExecFailed {
                            error: command_error
                        });
                        None
                    }
                },
                Err(_) => {
                    error!("Timed out during scheduled exec run.");
                    emit!(ExecTimeout {
                        elapsed_millis: now.elapsed().as_secs()
                    });
                    None
                }
            }
        }
        Err(spawn_error) => {
            error!("Error during scheduled exec run.");
            emit!(ExecFailed { error: spawn_error });
            None
        }
    }
}

fn process_scheduled_output(
    config: &ExecConfig,
    event_per_line: bool,
    exec_duration_millis_key: String,
    pid: u32,
    exit_status: Option<i32>,
    elapsed_millis: u128,
    output: Vec<u8>,
    stream: &str,
) -> Option<Vec<Event>> {
    let output_string: String = String::from_utf8(output).unwrap();

    if output_string.is_empty() {
        None
    } else {
        let mut events = Vec::new();
        if event_per_line {
            let lines = output_string.lines();
            for line in lines {
                let event = create_event(
                    config,
                    Bytes::from(line.to_owned()),
                    &Some(exec_duration_millis_key.clone()),
                    &Some(stream.to_string()),
                    Some(pid),
                    exit_status,
                    &Some(elapsed_millis),
                );

                events.push(event);
            }
        } else {
            let event = create_event(
                config,
                Bytes::from(output_string),
                &Some(exec_duration_millis_key),
                &Some(stream.to_string()),
                Some(pid),
                exit_status,
                &Some(elapsed_millis),
            );

            events.push(event);
        }

        Some(events)
    }
}

fn build_command(config: &ExecConfig) -> Command {
    let mut command = Command::new(config.command.as_str());
    command.kill_on_drop(true);

    // Explicitly set the current dir if needed
    if let Some(current_dir) = &config.current_dir {
        command.current_dir(current_dir);
    }

    // Pipe our stdout/stderr to the process to we inherit it's output
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    if let Some(arguments) = &config.arguments {
        if !arguments.is_empty() {
            command.args(arguments);
        }
    }

    command
}

fn create_event(
    config: &ExecConfig,
    line: Bytes,
    exec_duration_millis_key: &Option<String>,
    data_stream: &Option<String>,
    pid: Option<u32>,
    exit_status: Option<i32>,
    exec_duration_millis: &Option<u128>,
) -> Event {
    emit!(ExecEventReceived {
        byte_size: line.len()
    });
    let data_stream_key = config.data_stream_key.clone();
    let pid_key = config.pid_key.clone();
    let exit_status_key = config.exit_status_key.clone();
    let host_key = config.host_key.clone();
    let hostname = config.hostname.clone();
    let command_key = config.command_key.clone();
    let arguments_key = config.arguments_key.clone();
    let mut event = Event::from(line);
    let log_event = event.as_mut_log();

    // Add source type
    log_event.insert(log_schema().source_type_key(), Bytes::from(EXEC));

    // Add data stream of stdin or stderr (if needed)
    if let (Some(data_stream_key), Some(data_stream)) = (data_stream_key, data_stream) {
        log_event.insert(data_stream_key, data_stream.clone());
    }

    // Add pid (if needed)
    if let (Some(pid_key), Some(pid)) = (pid_key, pid) {
        log_event.insert(pid_key, pid as i64);
    }

    // Add exit status (if needed)
    if let (Some(exit_status_key), Some(exit_status)) = (exit_status_key, exit_status) {
        log_event.insert(exit_status_key, exit_status as i64);
    }

    // Add exec duration millis (if needed)
    if let (Some(exec_duration_millis_key), Some(exec_duration_millis)) =
        (exec_duration_millis_key, exec_duration_millis)
    {
        log_event.insert(exec_duration_millis_key, *exec_duration_millis as i64);
    }

    // Add hostname (if needed)
    if let Some(hostname) = hostname {
        log_event.insert(host_key, hostname);
    }

    // Add command (if needed)
    if let Some(command_key) = command_key {
        log_event.insert(command_key, config.command.clone());
    }

    // Add arguments (if needed)
    if let (Some(arguments_key), Some(arguments)) = (arguments_key, config.arguments.clone()) {
        log_event.insert(arguments_key, arguments);
    }

    event
}

fn spawn_streaming_thread<R: 'static + AsyncRead + Unpin + std::marker::Send>(
    lines: Lines<BufReader<R>>,
    shutdown: ShutdownSignal,
    stream: &'static str,
    mut sender: Sender<(String, &'static str)>,
) {
    // Start the green background thread for collecting
    tokio::spawn(async move {
        info!("Start capturing {} streaming command output.", stream);

        let mut lines_stream = lines.take_until(shutdown);

        while let Some(Ok(line)) = lines_stream.next().await {
            if sender.send((line, stream)).await.is_err() {
                break;
            }
        }

        info!("Finished capturing {} streaming command output.", stream);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::trace_init;
    use std::io::Cursor;

    #[test]
    fn test_generate_config() {
        crate::test_util::test_generate_config::<ExecConfig>();
    }

    #[test]
    fn test_scheduled_create_event() {
        let config = standard_scheduled_test_config();

        let exec_duration_millis_key = Some("exec_duration_millis".to_string());
        let line = Bytes::from("hello world");
        let data_stream = Some(STDOUT.to_string());
        let pid = Some(8888_u32);
        let exit_status = Some(0_i32);
        let exec_duration_millis = Some(500_u128);

        let event = create_event(
            &config,
            line,
            &exec_duration_millis_key,
            &data_stream,
            pid,
            exit_status,
            &exec_duration_millis,
        );
        let log = event.into_log();

        assert_eq!(log["host"], "Some.Machine".into());
        assert_eq!(log["data_stream"], STDOUT.into());
        assert_eq!(log["pid"], (8888_i64).into());
        assert_eq!(log["exit_status"], (0_i64).into());
        assert_eq!(log["command"], config.command.into());
        assert_eq!(log["arguments"], config.arguments.unwrap().into());
        assert_eq!(log["exec_duration_millis"], (500_i64).into());
        assert_eq!(log[log_schema().message_key()], "hello world".into());
        assert_eq!(log[log_schema().source_type_key()], "exec".into());
    }

    #[test]
    fn test_streaming_create_event() {
        let config = standard_streaming_test_config();

        let line = Bytes::from("hello world");
        let data_stream = Some(STDOUT.to_string());
        let pid = Some(8888_u32);
        let exit_status = None;
        let exec_duration_millis = None;

        let event = create_event(
            &config,
            line,
            &None,
            &data_stream,
            pid,
            exit_status,
            &exec_duration_millis,
        );
        let log = event.into_log();

        assert_eq!(log["host"], "Some.Machine".into());
        assert_eq!(log["data_stream"], STDOUT.into());
        assert_eq!(log["pid"], (8888_i64).into());
        assert_eq!(log["command"], config.command.clone().into());
        assert_eq!(log["arguments"], config.arguments.unwrap().into());
        assert_eq!(log[log_schema().message_key()], "hello world".into());
        assert_eq!(log[log_schema().source_type_key()], "exec".into());
    }

    #[test]
    fn test_build_command() {
        let config = ExecConfig {
            mode: Mode::Streaming {
                respawn_on_exit: default_respawn_on_exit(),
                respawn_interval_secs: default_respawn_interval_secs(),
            },
            command: "./runner".to_owned(),
            arguments: Some(vec!["arg1".to_owned(), "arg2".to_owned()]),
            current_dir: Some(PathBuf::from("/tmp")),
            include_stderr: None,
            host_key: default_host_key(),
            data_stream_key: None,
            pid_key: None,
            exit_status_key: None,
            command_key: None,
            arguments_key: None,
            hostname: None,
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
    fn test_process_scheduled_output_per_line() {
        let exec_duration_millis_key = "exec_duration_millis".to_string();
        let event_per_line = true;
        let config = standard_scheduled_test_config();

        let multiple_lines = "hello world\nhello world again".to_string().into_bytes();

        let data_stream = STDOUT;
        let pid = 8888_u32;
        let exit_status = Some(0_i32);
        let exec_duration_millis = 500_u128;

        let events = process_scheduled_output(
            &config,
            event_per_line,
            exec_duration_millis_key,
            pid,
            exit_status,
            exec_duration_millis,
            multiple_lines,
            data_stream,
        );

        let events = events.unwrap();
        assert_eq!(2, events.len());

        let event = events.get(0).unwrap();
        let log = event.as_log();
        assert_eq!(log["host"], "Some.Machine".into());
        assert_eq!(log["data_stream"], STDOUT.into());
        assert_eq!(log["pid"], (8888_i64).into());
        assert_eq!(log["exit_status"], (0_i64).into());
        assert_eq!(log["command"], config.command.clone().into());
        assert_eq!(log["arguments"], config.arguments.clone().unwrap().into());
        assert_eq!(log["exec_duration_millis"], (500_i64).into());
        assert_eq!(log[log_schema().message_key()], "hello world".into());
        assert_eq!(log[log_schema().source_type_key()], "exec".into());

        let event = events.get(1).unwrap();
        let log = event.as_log();
        assert_eq!(log["host"], "Some.Machine".into());
        assert_eq!(log["data_stream"], STDOUT.into());
        assert_eq!(log["pid"], (8888_i64).into());
        assert_eq!(log["exit_status"], (0_i64).into());
        assert_eq!(log["command"], config.command.clone().into());
        assert_eq!(log["arguments"], config.arguments.unwrap().into());
        assert_eq!(log["exec_duration_millis"], (500_i64).into());
        assert_eq!(log[log_schema().message_key()], "hello world again".into());
        assert_eq!(log[log_schema().source_type_key()], "exec".into());
    }

    #[test]
    fn test_process_scheduled_output_per_blob() {
        let exec_duration_millis_key = default_exec_duration_millis_key();
        let event_per_line = false;
        let config = standard_scheduled_test_config();

        let multiple_lines = "hello world\nhello world again".to_string().into_bytes();

        let data_stream = STDOUT;
        let pid = 8888_u32;
        let exit_status = Some(0_i32);
        let exec_duration_millis = 500_u128;

        let events = process_scheduled_output(
            &config,
            event_per_line,
            exec_duration_millis_key,
            pid,
            exit_status,
            exec_duration_millis,
            multiple_lines,
            data_stream,
        );

        let events = events.unwrap();
        assert_eq!(1, events.len());

        let event = events.get(0).unwrap();
        let log = event.as_log();
        assert_eq!(log["host"], "Some.Machine".into());
        assert_eq!(log["data_stream"], STDOUT.into());
        assert_eq!(log["pid"], (8888_i64).into());
        assert_eq!(log["exit_status"], (0_i64).into());
        assert_eq!(log["command"], config.command.clone().into());
        assert_eq!(log["arguments"], config.arguments.unwrap().into());
        assert_eq!(log["exec_duration_millis"], (500_i64).into());
        assert_eq!(
            log[log_schema().message_key()],
            "hello world\nhello world again".into()
        );
        assert_eq!(log[log_schema().source_type_key()], "exec".into());
    }

    #[tokio::test]
    async fn test_spawn_streaming_thread() {
        trace_init();

        let buf = Cursor::new("hello world\nhello world again");
        let reader = BufReader::new(buf);
        let lines = reader.lines();

        let shutdown = ShutdownSignal::noop();
        let (sender, mut receiver) = channel(1024);

        spawn_streaming_thread(lines, shutdown, STDOUT, sender);

        let mut counter = 0;
        if let Some((line, stream)) = receiver.recv().await {
            assert_eq!("hello world", line);
            assert_eq!(STDOUT, stream);
            counter += 1;
        }

        if let Some((line, stream)) = receiver.recv().await {
            assert_eq!("hello world again", line);
            assert_eq!(STDOUT, stream);
            counter += 1;
        }

        assert_eq!(counter, 2);
    }

    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_run_once_scheduled() {
        trace_init();

        let config = standard_scheduled_test_config();
        let exec_duration_millis_key = default_exec_duration_millis_key();
        let event_per_line = false;
        let exec_interval_secs = default_exec_interval_secs();

        let events = run_once_scheduled(
            &config,
            exec_interval_secs,
            event_per_line,
            exec_duration_millis_key,
        )
        .await;

        let events = events.unwrap();
        assert_eq!(1, events.len());

        let event = events.get(0).unwrap();
        let log = event.as_log();
        assert_eq!(log["host"], "Some.Machine".into());
        assert_eq!(log["data_stream"], STDOUT.into());
        assert_eq!(log["exit_status"], (0_i64).into());
        assert_eq!(log["command"], config.command.clone().into());
        assert_eq!(log[log_schema().source_type_key()], "exec".into());
        assert_eq!(log[log_schema().message_key()], "Hello World!\n".into());
        assert_eq!(log["arguments"], config.arguments.clone().unwrap().into());
    }

    fn standard_scheduled_test_config() -> ExecConfig {
        ExecConfig {
            mode: Mode::Scheduled {
                exec_interval_secs: default_exec_interval_secs(),
                event_per_line: default_events_per_line(),
                exec_duration_millis_key: default_exec_duration_millis_key(),
            },
            command: "echo".to_owned(),
            arguments: Some(vec!["Hello World!".to_owned()]),
            current_dir: None,
            include_stderr: Some(true),
            host_key: default_host_key(),
            data_stream_key: Some("data_stream".to_string()),
            pid_key: Some("pid".to_string()),
            exit_status_key: Some("exit_status".to_string()),
            command_key: Some("command".to_string()),
            arguments_key: Some("arguments".to_string()),
            hostname: Some("Some.Machine".to_string()),
        }
    }

    fn standard_streaming_test_config() -> ExecConfig {
        ExecConfig {
            mode: Mode::Streaming {
                respawn_on_exit: default_respawn_on_exit(),
                respawn_interval_secs: default_respawn_interval_secs(),
            },
            command: "streamer".to_owned(),
            arguments: Some(vec!["Hello World!".to_owned()]),
            current_dir: None,
            include_stderr: Some(true),
            host_key: default_host_key(),
            data_stream_key: Some("data_stream".to_string()),
            pid_key: Some("pid".to_string()),
            exit_status_key: Some("exit_status".to_string()),
            command_key: Some("command".to_string()),
            arguments_key: Some("arguments".to_string()),
            hostname: Some("Some.Machine".to_string()),
        }
    }
}
