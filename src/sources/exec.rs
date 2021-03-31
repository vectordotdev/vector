use crate::async_read::VecAsyncReadExt;
use crate::config::{DataType, GlobalOptions};
use crate::internal_events::{ExecCommandExecuted, ExecTimeout};
use crate::{
    config::{log_schema, SourceConfig, SourceDescription},
    event::Event,
    internal_events::{ExecEventReceived, ExecFailed},
    shutdown::ShutdownSignal,
    Pipeline,
};
use bytes::Bytes;
use derive_is_enum_variant::is_enum_variant;
use futures::{FutureExt, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::process::ExitStatus;
use std::task::Poll;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};
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
    #[serde(default = "default_events_per_line")]
    pub event_per_line: bool,
    #[serde(default = "default_maximum_buffer_size")]
    pub maximum_buffer_size: usize,
    #[serde(skip, default = "get_hostname")]
    pub hostname: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, is_enum_variant)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Scheduled {
        #[serde(default = "default_exec_interval_secs")]
        exec_interval_secs: u64,
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
            },
            command: "echo".to_owned(),
            arguments: Some(vec!["Hello World!".to_owned()]),
            current_dir: None,
            include_stderr: Some(true),
            event_per_line: default_events_per_line(),
            maximum_buffer_size: default_maximum_buffer_size(),
            hostname: get_hostname(),
        }
    }
}

fn default_maximum_buffer_size() -> usize {
    // 1GB
    1000000
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

fn default_events_per_line() -> bool {
    true
}

fn get_hostname() -> Option<String> {
    crate::get_hostname().ok()
}

pub const EXEC: &str = "exec";
pub const STDOUT: &str = "stdout";
pub const STDERR: &str = "stderr";
pub const DATA_STREAM_KEY: &str = "data_stream";
pub const PID_KEY: &str = "pid";
pub const EXIT_STATUS_KEY: &str = "exit_status";
pub const COMMAND_KEY: &str = "command";
pub const ARGUMENTS_KEY: &str = "arguments";
pub const EXEC_DURATION_MILLIS_KEY: &str = "exec_duration_millis";

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
            Mode::Scheduled { exec_interval_secs } => {
                run_scheduled(self.clone(), exec_interval_secs, shutdown, out)
            }
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
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> crate::Result<super::Source> {
    Ok(Box::pin(async move {
        info!("Starting scheduled exec runs.");
        let schedule = time::Duration::from_secs(exec_interval_secs);
        let mut interval = time::interval(schedule).take_until(shutdown.clone());

        while interval.next().await.is_some() {
            // Mark the start time just before spawning the process as
            // this seems to be the best approximation of exec duration
            let now = Instant::now();

            // Wait for our task to finish, wrapping it in a timeout
            let timeout = tokio::time::timeout(
                schedule,
                run_command(config.clone(), shutdown.clone(), out.clone()),
            );

            let timeout_result = timeout.await;

            match timeout_result {
                Ok(output) => {
                    if let Err(command_error) = output {
                        emit!(ExecFailed {
                            command: config.command.as_str(),
                            error: command_error,
                        });
                    }
                }
                Err(_) => {
                    emit!(ExecTimeout {
                        command: config.command.as_str(),
                        elapsed_seconds: now.elapsed().as_secs(),
                    });
                }
            }
        }

        info!("Finished scheduled exec runs.");
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
                let output = run_command(config.clone(), shutdown.clone(), out.clone()).await;

                if let Err(command_error) = output {
                    emit!(ExecFailed {
                        command: config.command.as_str(),
                        error: command_error,
                    });
                }

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
            let output = run_command(config.clone(), shutdown, out).await;

            if let Err(command_error) = output {
                emit!(ExecFailed {
                    command: config.command.as_str(),
                    error: command_error,
                });
            }
        }

        Ok(())
    }))
}

async fn run_command(
    config: ExecConfig,
    shutdown: ShutdownSignal,
    mut out: Pipeline,
) -> Result<Option<ExitStatus>, Error> {
    info!("Starting command run.");
    let mut command = build_command(&config);

    // Mark the start time just before spawning the process as
    // this seems to be the best approximation of exec duration
    let now = Instant::now();

    let mut child = command.spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::new(ErrorKind::Other, "Unable to take stdout of spawned process"))?;

    // Create stdout async reader
    let stdout_reader = BufReader::new(stdout);

    // Set up communication channels
    let (sender, mut receiver) = channel(1024);

    let pid = child.id();

    // Optionally include stderr
    if config.include_stderr.unwrap_or(true) {
        let stderr = child.stderr.take().ok_or_else(|| {
            Error::new(ErrorKind::Other, "Unable to take stderr of spawned process")
        })?;

        // Create stderr async reader
        let stderr_reader = BufReader::new(stderr);

        spawn_reader_thread(
            stderr_reader,
            shutdown.clone(),
            config.event_per_line,
            config.maximum_buffer_size,
            STDERR,
            sender.clone(),
        );
    }

    spawn_reader_thread(
        stdout_reader,
        shutdown.clone(),
        config.event_per_line,
        config.maximum_buffer_size,
        STDOUT,
        sender,
    );

    while let Some((line, stream)) = receiver.recv().await {
        let event = create_event(
            &config,
            Bytes::from(line),
            &Some(stream.to_string()),
            Some(pid),
            None,
            &None,
        );

        let _ = out
            .send(event)
            .await
            .map_err(|_: crate::pipeline::ClosedError| {
                error!(message = "Failed to forward events; downstream is closed.");
            });
    }

    let elapsed = now.elapsed();

    info!("Finished command run.");
    let _ = out.flush().await;

    // TODO: Tokio 1.0.1+ has a wait and try_wait method to get exit status
    if let Poll::Ready(Ok(exit_status)) = futures::poll!(child) {
        handle_exit_status(&config, pid, exit_status, elapsed, out).await;
        Ok(Some(exit_status))
    } else {
        emit!(ExecCommandExecuted {
            command: config.command.as_str(),
            exit_status: None,
            exec_duration: elapsed,
        });

        Ok(None)
    }
}

async fn handle_exit_status(
    config: &ExecConfig,
    pid: u32,
    exit_status: ExitStatus,
    exec_duration: Duration,
    mut out: Pipeline,
) {
    emit!(ExecCommandExecuted {
        command: config.command.as_str(),
        exit_status: exit_status.code(),
        exec_duration,
    });

    let event = create_event(
        config,
        Bytes::new(),
        &None,
        Some(pid),
        exit_status.code(),
        &Some(exec_duration.as_millis()),
    );

    let _ = out
        .send(event)
        .await
        .map_err(|_: crate::pipeline::ClosedError| {
            error!(message = "Failed to forward events; downstream is closed.");
        });
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
    data_stream: &Option<String>,
    pid: Option<u32>,
    exit_status: Option<i32>,
    exec_duration_millis: &Option<u128>,
) -> Event {
    emit!(ExecEventReceived {
        command: config.command.as_str(),
        byte_size: line.len(),
    });

    let mut event = Event::from(line);
    let log_event = event.as_mut_log();

    // Add source type
    log_event.insert(log_schema().source_type_key(), Bytes::from(EXEC));

    // Add data stream of stdin or stderr (if needed)
    if let Some(data_stream) = data_stream {
        log_event.insert(DATA_STREAM_KEY, data_stream.clone());
    }

    // Add pid (if needed)
    if let Some(pid) = pid {
        log_event.insert(PID_KEY, pid as i64);
    }

    // Add exit status (if needed)
    if let Some(exit_status) = exit_status {
        log_event.insert(EXIT_STATUS_KEY, exit_status as i64);
    }

    // Add exec duration millis (if needed)
    if let Some(exec_duration_millis) = exec_duration_millis {
        log_event.insert(EXEC_DURATION_MILLIS_KEY, *exec_duration_millis as i64);
    }

    // Add hostname (if needed)
    if let Some(hostname) = config.hostname.clone() {
        log_event.insert(log_schema().host_key(), hostname);
    }

    // Add command
    log_event.insert(COMMAND_KEY, config.command.clone());

    // Add arguments
    log_event.insert(ARGUMENTS_KEY, config.arguments.clone());

    event
}

fn spawn_reader_thread<R: 'static + AsyncRead + Unpin + std::marker::Send>(
    reader: BufReader<R>,
    shutdown: ShutdownSignal,
    event_per_line: bool,
    buf_size: usize,
    stream: &'static str,
    mut sender: Sender<(String, &'static str)>,
) {
    // Start the green background thread for collecting
    Box::pin(tokio::spawn(async move {
        info!("Start capturing {} command output.", stream);

        let mut buffer: Vec<u8> = Vec::new();

        let mut reader = reader.allow_read_until(shutdown.clone().map(|_| ()));

        // Read one byte at a time so we don't block waiting for the buffer to
        // fill up e.g. If a line filled up half the buffer we would not know about
        // it until the buffer is returned. We could increase the read buffer if we are
        // willing to accept some blocking waiting for it to fill up. I'm not sure if
        // the underlying BufReader will read in larger chunks or not.
        // Possibly could look into https://crates.io/crates/fixed-buffer-tokio but
        // I don't know much about this library.
        let mut read_buffer = [0_u8; 1];

        // Not using the shutdown signal in this method so we need to shutdown
        // in other methods to end the reading
        while let Ok(bytes_read) = reader.read(&mut read_buffer).await {
            if bytes_read == 0 {
                info!("End of input reached, stop reading");
                break;
            } else {
                let read_byte = read_buffer[0];

                // Could be enhanced to split 'lines' based on user defined
                // delimiters in addition to newline.
                if event_per_line && read_byte == b'\n' {
                    if buffer.ends_with(&[b'\r']) {
                        let _ = buffer.pop();
                    }

                    if let Some(buffer_string) = buffer_to_string(&mut buffer, false) {
                        if sender.send((buffer_string, stream)).await.is_err() {
                            break;
                        }
                    } else {
                        info!("Invalid utf8, stop reading");
                        break;
                    }
                } else {
                    buffer.push(read_byte);

                    if buffer.len() == buf_size {
                        if let Some(buffer_string) = buffer_to_string(&mut buffer, true) {
                            if sender.send((buffer_string, stream)).await.is_err() {
                                break;
                            }
                        } else {
                            info!("Invalid utf8, stop reading");
                            break;
                        }
                    }
                }
            }
        }

        // Handle any left over buffer
        if !buffer.is_empty() {
            if let Some(buffer_string) = buffer_to_string(&mut buffer, true) {
                let _ = sender.send((buffer_string, stream)).await;
                if !buffer.is_empty() {
                    info!("Invalid utf8, left in buffer");
                }
            } else {
                info!("Invalid utf8, left in buffer");
            }
        }

        info!("Finished capturing {} command output.", stream);
    }));
}

fn buffer_to_string(buffer: &mut Vec<u8>, allow_shrinking: bool) -> Option<String> {
    let mut left_over_buffer: Vec<u8> = Vec::new();
    loop {
        if let Ok(buffer_string) = String::from_utf8(buffer.clone()) {
            buffer.clear();
            buffer.append(&mut left_over_buffer);
            return Some(buffer_string);
        } else {
            // Only try shrinking the buffer by at most 3 bytes as
            // the maximum utf8 character is 4 bytes. If we shrink
            // by 3 and it is still invalid then assume the whole thing
            // is invalid utf8. Don't shrink to smaller than 1 byte.
            if allow_shrinking && left_over_buffer.len() < 3 && buffer.len() > 1 {
                if let Some(last_byte) = buffer.pop() {
                    left_over_buffer.insert(0, last_byte);
                } else {
                    buffer.append(&mut left_over_buffer);
                    return None;
                }
            } else {
                buffer.append(&mut left_over_buffer);
                return None;
            }
        }
    }
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
        let line = Bytes::from("hello world");
        let data_stream = Some(STDOUT.to_string());
        let pid = Some(8888_u32);
        let exit_status = None;
        let exec_duration_millis = None;

        let event = create_event(
            &config,
            line,
            &data_stream,
            pid,
            exit_status,
            &exec_duration_millis,
        );
        let log = event.into_log();

        assert_eq!(log[log_schema().host_key()], "Some.Machine".into());
        assert_eq!(log[DATA_STREAM_KEY], STDOUT.into());
        assert_eq!(log[PID_KEY], (8888_i64).into());
        assert_eq!(log[COMMAND_KEY], config.command.clone().into());
        assert_eq!(log[ARGUMENTS_KEY], config.arguments.unwrap().into());
        assert_eq!(log[log_schema().message_key()], "hello world".into());
        assert_eq!(log[log_schema().source_type_key()], "exec".into());
        assert_ne!(log[log_schema().timestamp_key()], "".into());
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
            &data_stream,
            pid,
            exit_status,
            &exec_duration_millis,
        );
        let log = event.into_log();

        assert_eq!(log[log_schema().host_key()], "Some.Machine".into());
        assert_eq!(log[DATA_STREAM_KEY], STDOUT.into());
        assert_eq!(log[PID_KEY], (8888_i64).into());
        assert_eq!(log[COMMAND_KEY], config.command.clone().into());
        assert_eq!(log[ARGUMENTS_KEY], config.arguments.unwrap().into());
        assert_eq!(log[log_schema().message_key()], "hello world".into());
        assert_eq!(log[log_schema().source_type_key()], "exec".into());
        assert_ne!(log[log_schema().timestamp_key()], "".into());
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
            event_per_line: true,
            maximum_buffer_size: default_maximum_buffer_size(),
            hostname: get_hostname(),
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
    fn test_buffer_to_string_no_leftover() {
        let mut buffer: Vec<u8> = vec![0x46, 0x61, 0x73, 0x74, 0x20, 0xF0, 0x9F, 0x9A, 0x80];

        if let Some(buffer_string) = buffer_to_string(&mut buffer, false) {
            assert_eq!("Fast 🚀", buffer_string);
            assert_eq!(0, buffer.len());
        } else {
            panic!("The buffer should be converted to a string");
        }
    }

    #[test]
    fn test_buffer_to_string_with_leftover() {
        let mut buffer: Vec<u8> = vec![
            0x46, 0x61, 0x73, 0x74, 0x20, 0xF0, 0x9F, 0x9A, 0x80, 0xF0, 0x9F,
        ];

        if let Some(buffer_string) = buffer_to_string(&mut buffer, true) {
            assert_eq!("Fast 🚀", buffer_string);
            assert_eq!(2, buffer.len());
            assert_eq!(vec![0xF0, 0x9F], buffer);
        } else {
            panic!("The buffer should be converted to a string");
        }
    }

    #[test]
    fn test_buffer_to_string_invalid_utf8() {
        let mut buffer: Vec<u8> = vec![
            0x46, 0x61, 0x73, 0x74, 0x20, 0xF0, 0x9F, 0x9A, 0x80, 0xF0, 0x9F,
        ];

        if buffer_to_string(&mut buffer, false).is_some() {
            panic!("The buffer should be not converted to a string");
        } else {
            assert_eq!(11, buffer.len());
        }
    }

    #[tokio::test]
    async fn test_spawn_reader_thread_per_line() {
        trace_init();

        let buf = Cursor::new("hello world\nhello rocket 🚀");
        let reader = BufReader::new(buf);
        let shutdown = ShutdownSignal::noop();
        let (sender, mut receiver) = channel(1024);

        spawn_reader_thread(reader, shutdown, true, 88888, STDOUT, sender);

        let mut counter = 0;
        if let Some((line, stream)) = receiver.recv().await {
            assert_eq!("hello world", line);
            assert_eq!(STDOUT, stream);
            counter += 1;
        }

        if let Some((line, stream)) = receiver.recv().await {
            assert_eq!("hello rocket 🚀", line);
            assert_eq!(STDOUT, stream);
            counter += 1;
        }

        assert_eq!(counter, 2);
    }

    #[tokio::test]
    async fn test_spawn_reader_thread_per_blob() {
        trace_init();

        let buf = Cursor::new("hello world\nhello rocket 🚀");
        let reader = BufReader::new(buf);
        let shutdown = ShutdownSignal::noop();
        let (sender, mut receiver) = channel(1024);

        spawn_reader_thread(reader, shutdown, false, 88888, STDOUT, sender);

        let mut counter = 0;
        if let Some((line, stream)) = receiver.recv().await {
            assert_eq!("hello world\nhello rocket 🚀", line);
            assert_eq!(STDOUT, stream);
            counter += 1;
        }

        assert_eq!(counter, 1);
    }

    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_run_command_linux() {
        trace_init();
        let config = standard_scheduled_test_config();
        let (tx, mut rx) = Pipeline::new_test();
        let shutdown = ShutdownSignal::noop();

        // Wait for our task to finish, wrapping it in a timeout
        let timeout = tokio::time::timeout(
            time::Duration::from_secs(5),
            run_command(config.clone(), shutdown, tx),
        );

        let timeout_result = timeout.await;

        match timeout_result {
            Ok(output) => match output {
                Ok(exit_status) => assert_eq!(0_i32, exit_status.unwrap().code().unwrap()),
                Err(_) => panic!("Unable to run linux command"),
            },
            Err(_) => panic!("Timed out during test of run linux command."),
        }

        if let Ok(event) = rx.try_recv() {
            let log = event.as_log();
            assert_eq!(log[COMMAND_KEY], config.command.clone().into());
            assert_eq!(log[ARGUMENTS_KEY], config.arguments.clone().unwrap().into());
            assert_eq!(log[DATA_STREAM_KEY], STDOUT.into());
            assert_eq!(log[log_schema().source_type_key()], "exec".into());
            assert_eq!(log[log_schema().message_key()], "Hello World!".into());
            assert_eq!(log[log_schema().host_key()], "Some.Machine".into());
            assert_ne!(log[PID_KEY], "".into());
            assert_ne!(log[log_schema().timestamp_key()], "".into());

            let mut counter = 0;
            for _ in log.all_fields() {
                counter += 1;
            }

            assert_eq!(8, counter);
        } else {
            panic!("Expected to receive a linux event");
        }

        if let Ok(event) = rx.try_recv() {
            let log = event.as_log();
            assert_eq!(log[COMMAND_KEY], config.command.clone().into());
            assert_eq!(log[ARGUMENTS_KEY], config.arguments.clone().unwrap().into());
            assert_eq!(log[log_schema().source_type_key()], "exec".into());
            assert_eq!(log[log_schema().message_key()], "".into());
            assert_eq!(log[log_schema().host_key()], "Some.Machine".into());
            assert_eq!(log[EXIT_STATUS_KEY], (0_i64).into());
            assert_ne!(log[PID_KEY], "".into());
            assert_ne!(log[EXEC_DURATION_MILLIS_KEY], "".into());
            assert_ne!(log[log_schema().timestamp_key()], "".into());

            let mut counter = 0;
            for _ in log.all_fields() {
                counter += 1;
            }

            assert_eq!(9, counter);
        } else {
            panic!("Expected to receive an end of process linux event");
        }
    }

    #[tokio::test]
    #[cfg(target_os = "windows")]
    async fn test_run_command_windows() {
        trace_init();
        let config = standard_scheduled_windows_test_config();

        let (tx, mut rx) = Pipeline::new_test();
        let shutdown = ShutdownSignal::noop();

        // Wait for our task to finish, wrapping it in a timeout
        let timeout = tokio::time::timeout(
            time::Duration::from_secs(5),
            run_command(config.clone(), shutdown, tx),
        );

        let timeout_result = timeout.await;

        match timeout_result {
            Ok(output) => match output {
                Ok(exit_status) => assert_eq!(0_i32, exit_status.unwrap().code().unwrap()),
                Err(_) => panic!("Unable to run windows command"),
            },
            Err(_) => panic!("Timed out during test of run windows command."),
        }

        if let Ok(event) = rx.try_recv() {
            let log = event.as_log();
            assert_eq!(log[COMMAND_KEY], config.command.clone().into());
            assert_eq!(log[DATA_STREAM_KEY], STDOUT.into());
            assert_eq!(log[log_schema().source_type_key()], "exec".into());
            assert_ne!(log[log_schema().message_key()], "".into());
            assert_eq!(log[log_schema().host_key()], "Some.Machine".into());
            assert_ne!(log[PID_KEY], "".into());
            assert_ne!(log[log_schema().timestamp_key()], "".into());

            let mut counter = 0;
            for _ in log.all_fields() {
                counter += 1;
            }

            assert_eq!(8, counter);
        } else {
            panic!("Expected to receive a windows event");
        }

        if let Ok(event) = rx.try_recv() {
            let log = event.as_log();
            assert_eq!(log[COMMAND_KEY], config.command.clone().into());
            assert_eq!(log[log_schema().source_type_key()], "exec".into());
            assert_eq!(log[log_schema().message_key()], "".into());
            assert_eq!(log[log_schema().host_key()], "Some.Machine".into());
            assert_eq!(log[EXIT_STATUS_KEY], (0_i64).into());
            assert_ne!(log[PID_KEY], "".into());
            assert_ne!(log[EXEC_DURATION_MILLIS_KEY], "".into());
            assert_ne!(log[log_schema().timestamp_key()], "".into());

            let mut counter = 0;
            for _ in log.all_fields() {
                counter += 1;
            }

            assert_eq!(9, counter);
        } else {
            panic!("Expected to receive an end of process windows event");
        }
    }

    #[cfg(target_os = "windows")]
    fn standard_scheduled_windows_test_config() -> ExecConfig {
        ExecConfig {
            mode: Mode::Scheduled {
                exec_interval_secs: default_exec_interval_secs(),
            },
            command: "dir".to_owned(),
            arguments: None,
            current_dir: None,
            include_stderr: Some(true),
            event_per_line: false,
            maximum_buffer_size: default_maximum_buffer_size(),
            hostname: Some("Some.Machine".to_string()),
        }
    }

    fn standard_scheduled_test_config() -> ExecConfig {
        ExecConfig {
            mode: Mode::Scheduled {
                exec_interval_secs: default_exec_interval_secs(),
            },
            command: "echo".to_owned(),
            arguments: Some(vec!["Hello World!".to_owned()]),
            current_dir: None,
            include_stderr: Some(true),
            event_per_line: true,
            maximum_buffer_size: default_maximum_buffer_size(),
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
            event_per_line: true,
            maximum_buffer_size: default_maximum_buffer_size(),
            hostname: Some("Some.Machine".to_string()),
        }
    }
}
