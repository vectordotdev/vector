use crate::async_buf_read::VecAsyncBufReadExt;
use crate::config::{DataType, GlobalOptions};
use crate::event::LogEvent;
use crate::internal_events::{ExecCommandExecuted, ExecTimeout};
use crate::{
    config::{log_schema, SourceConfig, SourceDescription},
    event::Event,
    internal_events::{ExecEventReceived, ExecFailed},
    shutdown::ShutdownSignal,
    Pipeline,
};
use bytes::Bytes;
use chrono::Utc;
use futures::{FutureExt, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::process::ExitStatus;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::{channel, Sender};
use tokio::time::{self, sleep, Duration, Instant};
use tokio_stream::wrappers::IntervalStream;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct ExecConfig {
    pub mode: Mode,
    pub scheduled: Option<ScheduledConfig>,
    pub streaming: Option<StreamingConfig>,
    pub command: Vec<String>,
    pub working_directory: Option<PathBuf>,
    #[serde(default = "default_include_stderr")]
    pub include_stderr: bool,
    #[serde(default = "default_events_per_line")]
    pub event_per_line: bool,
    #[serde(default = "default_maximum_buffer_size")]
    pub maximum_buffer_size_bytes: u64,
}

// TODO: Would be nice to combine the scheduled and streaming config with the mode enum once
//       this serde ticket has been addressed (https://github.com/serde-rs/serde/issues/2013)
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Mode {
    Scheduled,
    Streaming,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ScheduledConfig {
    #[serde(default = "default_exec_interval_secs")]
    exec_interval_secs: u64,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct StreamingConfig {
    #[serde(default = "default_respawn_on_exit")]
    respawn_on_exit: bool,
    #[serde(default = "default_respawn_interval_secs")]
    respawn_interval_secs: u64,
}

#[derive(Debug, PartialEq, Snafu)]
pub enum ExecConfigError {
    #[snafu(display("A non-empty list for command must be provided"))]
    CommandEmpty,
    #[snafu(display("The maximum buffer size must be greater than zero"))]
    ZeroBuffer,
}

impl Default for ExecConfig {
    fn default() -> Self {
        ExecConfig {
            mode: Mode::Scheduled,
            scheduled: Some(ScheduledConfig {
                exec_interval_secs: default_exec_interval_secs(),
            }),
            streaming: None,
            command: vec!["echo".to_owned(), "Hello World!".to_owned()],
            working_directory: None,
            include_stderr: default_include_stderr(),
            event_per_line: default_events_per_line(),
            maximum_buffer_size_bytes: default_maximum_buffer_size(),
        }
    }
}

fn default_maximum_buffer_size() -> u64 {
    // 1MB
    1000000
}

fn default_exec_interval_secs() -> u64 {
    60
}

fn default_respawn_interval_secs() -> u64 {
    5
}

fn default_respawn_on_exit() -> bool {
    true
}

fn default_include_stderr() -> bool {
    true
}

fn default_events_per_line() -> bool {
    true
}

fn get_hostname() -> Option<String> {
    crate::get_hostname().ok()
}

const EXEC: &str = "exec";
const STDOUT: &str = "stdout";
const STDERR: &str = "stderr";
const STREAM_KEY: &str = "stream";
const PID_KEY: &str = "pid";
const EXIT_STATUS_KEY: &str = "exit_status";
const COMMAND_KEY: &str = "command";
const EXEC_DURATION_MILLIS_KEY: &str = "exec_duration_millis";

inventory::submit! {
    SourceDescription::new::<ExecConfig>("exec")
}

impl_generate_config_from_default!(ExecConfig);

impl ExecConfig {
    pub(self) fn validate(&self) -> Result<(), ExecConfigError> {
        if self.command.is_empty() {
            Err(ExecConfigError::CommandEmpty)
        } else if self.maximum_buffer_size_bytes == 0 {
            Err(ExecConfigError::ZeroBuffer)
        } else {
            Ok(())
        }
    }

    pub(self) fn command_line(&self) -> String {
        self.command.join(" ")
    }

    pub(self) fn exec_interval_secs_or_default(&self) -> u64 {
        match &self.scheduled {
            None => default_exec_interval_secs(),
            Some(config) => config.exec_interval_secs,
        }
    }

    pub(self) fn respawn_on_exit_or_default(&self) -> bool {
        match &self.streaming {
            None => default_respawn_on_exit(),
            Some(config) => config.respawn_on_exit,
        }
    }

    pub(self) fn respawn_interval_secs_or_default(&self) -> u64 {
        match &self.streaming {
            None => default_respawn_interval_secs(),
            Some(config) => config.respawn_interval_secs,
        }
    }
}

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
        self.validate()?;
        let hostname = get_hostname();
        match self.mode.clone() {
            Mode::Scheduled => {
                let exec_interval_secs = self.exec_interval_secs_or_default();
                run_scheduled(self.clone(), hostname, exec_interval_secs, shutdown, out)
            }
            Mode::Streaming => {
                let respawn_on_exit = self.respawn_on_exit_or_default();
                let respawn_interval_secs = self.respawn_interval_secs_or_default();
                run_streaming(
                    self.clone(),
                    hostname,
                    respawn_on_exit,
                    respawn_interval_secs,
                    shutdown,
                    out,
                )
            }
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
    hostname: Option<String>,
    exec_interval_secs: u64,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> crate::Result<super::Source> {
    Ok(Box::pin(async move {
        debug!("Starting scheduled exec runs.");
        let schedule = Duration::from_secs(exec_interval_secs);

        let mut interval =
            IntervalStream::new(time::interval(schedule)).take_until(shutdown.clone());

        while interval.next().await.is_some() {
            // Mark the start time just before spawning the process as
            // this seems to be the best approximation of exec duration
            let now = Instant::now();

            // Wait for our task to finish, wrapping it in a timeout
            let timeout = tokio::time::timeout(
                schedule,
                run_command(
                    config.clone(),
                    hostname.clone(),
                    shutdown.clone(),
                    out.clone(),
                ),
            );

            let timeout_result = timeout.await;

            match timeout_result {
                Ok(output) => {
                    if let Err(command_error) = output {
                        emit!(ExecFailed {
                            command: config.command_line().as_str(),
                            error: command_error,
                        });
                    }
                }
                Err(_) => {
                    emit!(ExecTimeout {
                        command: config.command_line().as_str(),
                        elapsed_seconds: now.elapsed().as_secs(),
                    });
                }
            }
        }

        debug!("Finished scheduled exec runs.");
        Ok(())
    }))
}

pub fn run_streaming(
    config: ExecConfig,
    hostname: Option<String>,
    respawn_on_exit: bool,
    respawn_interval_secs: u64,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> crate::Result<super::Source> {
    Ok(Box::pin(async move {
        if respawn_on_exit {
            let duration = Duration::from_secs(respawn_interval_secs);

            // Continue to loop while not shutdown
            loop {
                tokio::select! {
                    _ = shutdown.clone() => break, // will break early if a shutdown is started
                    output = run_command(config.clone(), hostname.clone(), shutdown.clone(), out.clone()) => {
                        // handle command finished
                        if let Err(command_error) = output {
                            emit!(ExecFailed {
                                command: config.command_line().as_str(),
                                error: command_error,
                            });
                        }
                    }
                }

                let mut poll_shutdown = shutdown.clone();
                if futures::poll!(&mut poll_shutdown).is_pending() {
                    warn!("Streaming process ended before shutdown.");
                }

                tokio::select! {
                    _ = &mut poll_shutdown => break, // will break early if a shutdown is started
                    _ = sleep(duration) => debug!("Restarting streaming process."),
                }
            }
        } else {
            let output = run_command(config.clone(), hostname, shutdown, out).await;

            if let Err(command_error) = output {
                emit!(ExecFailed {
                    command: config.command_line().as_str(),
                    error: command_error,
                });
            }
        }

        Ok(())
    }))
}

async fn run_command(
    config: ExecConfig,
    hostname: Option<String>,
    shutdown: ShutdownSignal,
    mut out: Pipeline,
) -> Result<Option<ExitStatus>, Error> {
    debug!("Starting command run.");
    let mut command = build_command(&config);

    // Mark the start time just before spawning the process as
    // this seems to be the best approximation of exec duration
    let start = Instant::now();

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
    if config.include_stderr {
        let stderr = child.stderr.take().ok_or_else(|| {
            Error::new(ErrorKind::Other, "Unable to take stderr of spawned process")
        })?;

        // Create stderr async reader
        let stderr_reader = BufReader::new(stderr);

        spawn_reader_thread(
            stderr_reader,
            shutdown.clone(),
            config.event_per_line,
            config.maximum_buffer_size_bytes,
            STDERR,
            sender.clone(),
        );
    }

    spawn_reader_thread(
        stdout_reader,
        shutdown.clone(),
        config.event_per_line,
        config.maximum_buffer_size_bytes,
        STDOUT,
        sender,
    );

    while let Some((line, stream)) = receiver.recv().await {
        let event = create_event(
            &config,
            &hostname,
            line,
            &Some(stream.to_string()),
            pid,
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

    let elapsed = start.elapsed();

    debug!("Finished command run.");
    let _ = out.flush().await;

    match child.try_wait() {
        Ok(Some(exit_status)) => {
            handle_exit_status(&config, Some(exit_status), elapsed).await;
            Ok(Some(exit_status))
        }
        Ok(None) => {
            handle_exit_status(&config, None, elapsed).await;
            Ok(None)
        }
        Err(error) => {
            error!(message = "Unable to obtain exit status.", %error);

            handle_exit_status(&config, None, elapsed).await;
            Ok(None)
        }
    }
}

async fn handle_exit_status(
    config: &ExecConfig,
    exit_status: Option<ExitStatus>,
    exec_duration: Duration,
) {
    let exit_status = match exit_status {
        Some(exit_status) => exit_status.code(),
        None => None,
    };

    emit!(ExecCommandExecuted {
        command: config.command_line().as_str(),
        exit_status,
        exec_duration,
    });
}

fn build_command(config: &ExecConfig) -> Command {
    let command = &config.command[0];

    let mut command = Command::new(command);

    if config.command.len() > 1 {
        command.args(&config.command[1..]);
    };

    command.kill_on_drop(true);

    // Explicitly set the current dir if needed
    if let Some(current_dir) = &config.working_directory {
        command.current_dir(current_dir);
    }

    // Pipe our stdout/stderr to the process to we inherit it's output
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    command
}

fn create_event(
    config: &ExecConfig,
    hostname: &Option<String>,
    line: Bytes,
    data_stream: &Option<String>,
    pid: Option<u32>,
    exit_status: Option<i32>,
    exec_duration_millis: &Option<u128>,
) -> Event {
    emit!(ExecEventReceived {
        command: config.command_line().as_str(),
        byte_size: line.len(),
    });
    let mut log_event = LogEvent::default();

    // Add message
    log_event.insert(log_schema().message_key(), line);

    // Add timestamp
    log_event.insert(log_schema().timestamp_key(), Utc::now());

    // Add source type
    log_event.insert(log_schema().source_type_key(), Bytes::from(EXEC));

    // Add data stream of stdin or stderr (if needed)
    if let Some(data_stream) = data_stream {
        log_event.insert(STREAM_KEY, data_stream.clone());
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
    if let Some(hostname) = hostname {
        log_event.insert(log_schema().host_key(), hostname.clone());
    }

    // Add command
    log_event.insert(COMMAND_KEY, config.command.clone());

    Event::Log(log_event)
}

fn spawn_reader_thread<R: 'static + AsyncRead + Unpin + std::marker::Send>(
    reader: BufReader<R>,
    shutdown: ShutdownSignal,
    event_per_line: bool,
    buf_size: u64,
    stream: &'static str,
    sender: Sender<(Bytes, &'static str)>,
) {
    // Start the green background thread for collecting
    Box::pin(tokio::spawn(async move {
        debug!("Start capturing {} command output.", stream);

        let mut read_buffer: Vec<u8> = Vec::new();

        let reader = reader.allow_read_until(shutdown.clone().map(|_| ()));

        let mut limited_reader = reader.take(buf_size);

        if event_per_line {
            // Keep reading lines (lines longer than the max buffer will be split)
            while let Ok(bytes_read) = limited_reader.read_until(b'\n', &mut read_buffer).await {
                if bytes_read == 0 {
                    // If we get a continuous stream of \n the bytes_read will be at least 1
                    debug!("End of input reached, stop reading.");
                    break;
                }

                // Strip of the end of line bytes
                if read_buffer.ends_with(&[b'\n']) {
                    let _ = read_buffer.pop();

                    if read_buffer.ends_with(&[b'\r']) {
                        let _ = read_buffer.pop();
                    }
                }

                let read_bytes = Bytes::from(read_buffer.clone());
                if sender.send((read_bytes, stream)).await.is_err() {
                    // If the receive half of the channel is closed, either due to close being
                    // called or the Receiver handle dropping, the function returns an error.
                    debug!("Receive channel closed, unable to send.");
                    break;
                }

                // Clear the read buffer ready for the next read
                read_buffer.clear();

                // Reset the limit for the next read
                limited_reader.set_limit(buf_size);
            }
        } else {
            // Keep reading max buffer chunks
            while let Ok(bytes_read) = limited_reader.read_to_end(&mut read_buffer).await {
                if bytes_read == 0 {
                    debug!("End of input reached, stop reading.");
                    break;
                }

                let read_bytes = Bytes::from(read_buffer.clone());
                if sender.send((read_bytes, stream)).await.is_err() {
                    // If the receive half of the channel is closed, either due to close being
                    // called or the Receiver handle dropping, the function returns an error.
                    debug!("Receive channel closed, unable to send.");
                    break;
                }

                // Clear the read buffer ready for the next read
                read_buffer.clear();

                // Reset the limit for the next read
                limited_reader.set_limit(buf_size);
            }
        }

        // Handle any left over buffer
        if !read_buffer.is_empty() {
            let read_bytes = Bytes::from(read_buffer.clone());
            if sender.send((read_bytes, stream)).await.is_err() {
                debug!("Receive channel closed, unable to send.");
            }
        }

        debug!("Finished capturing {} command output.", stream);
    }));
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
        let hostname = Some("Some.Machine".to_string());
        let line = Bytes::from("hello world");
        let data_stream = Some(STDOUT.to_string());
        let pid = Some(8888_u32);
        let exit_status = None;
        let exec_duration_millis = None;

        let event = create_event(
            &config,
            &hostname,
            line,
            &data_stream,
            pid,
            exit_status,
            &exec_duration_millis,
        );
        let log = event.into_log();

        assert_eq!(log[log_schema().host_key()], "Some.Machine".into());
        assert_eq!(log[STREAM_KEY], STDOUT.into());
        assert_eq!(log[PID_KEY], (8888_i64).into());
        assert_eq!(log[COMMAND_KEY], config.command.into());
        assert_eq!(log[log_schema().message_key()], "hello world".into());
        assert_eq!(log[log_schema().source_type_key()], "exec".into());
        assert_ne!(log[log_schema().timestamp_key()], "".into());
    }

    #[test]
    fn test_streaming_create_event() {
        let config = standard_streaming_test_config();
        let hostname = Some("Some.Machine".to_string());
        let line = Bytes::from("hello world");
        let data_stream = Some(STDOUT.to_string());
        let pid = Some(8888_u32);
        let exit_status = None;
        let exec_duration_millis = None;

        let event = create_event(
            &config,
            &hostname,
            line,
            &data_stream,
            pid,
            exit_status,
            &exec_duration_millis,
        );
        let log = event.into_log();

        assert_eq!(log[log_schema().host_key()], "Some.Machine".into());
        assert_eq!(log[STREAM_KEY], STDOUT.into());
        assert_eq!(log[PID_KEY], (8888_i64).into());
        assert_eq!(log[COMMAND_KEY], config.command.into());
        assert_eq!(log[log_schema().message_key()], "hello world".into());
        assert_eq!(log[log_schema().source_type_key()], "exec".into());
        assert_ne!(log[log_schema().timestamp_key()], "".into());
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
            working_directory: Some(PathBuf::from("/tmp")),
            include_stderr: default_include_stderr(),
            event_per_line: default_events_per_line(),
            maximum_buffer_size_bytes: default_maximum_buffer_size(),
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
            assert_eq!(Bytes::from("hello world"), line);
            assert_eq!(STDOUT, stream);
            counter += 1;
        }

        if let Some((line, stream)) = receiver.recv().await {
            assert_eq!(Bytes::from("hello rocket 🚀"), line);
            assert_eq!(STDOUT, stream);
            counter += 1;
        }

        assert_eq!(counter, 2);
    }

    #[tokio::test]
    async fn test_spawn_reader_thread_per_line_tiny_buffer() {
        trace_init();

        let buf = Cursor::new("hello world\n🚀 123");
        let reader = BufReader::new(buf);
        let shutdown = ShutdownSignal::noop();
        let (sender, mut receiver) = channel(1024);

        spawn_reader_thread(reader, shutdown, true, 6, STDOUT, sender);

        let mut counter = 0;
        if let Some((line, stream)) = receiver.recv().await {
            assert_eq!(Bytes::from("hello "), line);
            assert_eq!(STDOUT, stream);
            counter += 1;
        }

        if let Some((line, stream)) = receiver.recv().await {
            assert_eq!(Bytes::from("world"), line);
            assert_eq!(STDOUT, stream);
            counter += 1;
        }

        if let Some((line, stream)) = receiver.recv().await {
            assert_eq!(Bytes::from("🚀 1"), line);
            assert_eq!(STDOUT, stream);
            counter += 1;
        }

        if let Some((line, stream)) = receiver.recv().await {
            assert_eq!(Bytes::from("23"), line);
            assert_eq!(STDOUT, stream);
            counter += 1;
        }

        assert_eq!(counter, 4);
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
            assert_eq!(Bytes::from("hello world\nhello rocket 🚀"), line);
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
        let hostname = Some("Some.Machine".to_string());
        let (tx, mut rx) = Pipeline::new_test();
        let shutdown = ShutdownSignal::noop();

        // Wait for our task to finish, wrapping it in a timeout
        let timeout = tokio::time::timeout(
            time::Duration::from_secs(5),
            run_command(config.clone(), hostname, shutdown, tx),
        );

        let timeout_result = timeout.await;

        let exit_status = timeout_result
            .expect("command timed out")
            .expect("command error");
        assert_eq!(0_i32, exit_status.unwrap().code().unwrap());

        if let Ok(Some(event)) = rx.try_next() {
            let log = event.as_log();
            assert_eq!(log[COMMAND_KEY], config.command.clone().into());
            assert_eq!(log[STREAM_KEY], STDOUT.into());
            assert_eq!(log[log_schema().source_type_key()], "exec".into());
            assert_eq!(log[log_schema().message_key()], "Hello World!".into());
            assert_eq!(log[log_schema().host_key()], "Some.Machine".into());
            assert_ne!(log[PID_KEY], "".into());
            assert_ne!(log[log_schema().timestamp_key()], "".into());

            assert_eq!(8, log.all_fields().count());
        } else {
            panic!("Expected to receive a linux event");
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
            working_directory: None,
            include_stderr: default_include_stderr(),
            event_per_line: default_events_per_line(),
            maximum_buffer_size_bytes: default_maximum_buffer_size(),
        }
    }
}
