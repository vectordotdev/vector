use std::{collections::HashMap, path::PathBuf, time::Duration};

use vector_config::configurable_component;
use vector_lib::codecs::{
    JsonSerializerConfig,
    decoding::{self, DeserializerConfig},
};

use crate::{
    codecs::{EncodingConfigWithFraming, Transformer},
    serde::default_decoding,
};

/// Configuration for the `stdio` transform.
#[configurable_component(transform("stdio", "Transform events via an external process."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct StdioConfig {
    #[configurable(derived)]
    #[serde(default)]
    pub mode: Mode,

    #[configurable(derived)]
    pub scheduled: Option<ScheduledConfig>,

    #[configurable(derived)]
    pub streaming: Option<StreamingConfig>,

    #[configurable(derived)]
    pub per_event: Option<PerEventConfig>,

    #[configurable(derived)]
    #[serde(flatten)]
    pub command: CommandConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub stdin: StdinConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub stdout: StdoutConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub stderr: StderrConfig,
}

impl Default for StdioConfig {
    fn default() -> Self {
        StdioConfig {
            mode: Mode::Scheduled,
            scheduled: Some(ScheduledConfig::default()),
            streaming: Some(StreamingConfig::default()),
            per_event: Some(PerEventConfig::default()),
            command: CommandConfig::default(),
            stdin: StdinConfig::default(),
            stdout: StdoutConfig::default(),
            stderr: StderrConfig::default(),
        }
    }
}

impl_generate_config_from_default!(StdioConfig);

/// Configuration options for scheduled commands.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct ScheduledConfig {
    /// The interval, in seconds, between scheduled command runs.
    ///
    /// If the command takes longer than `exec_interval_secs` to run, it is
    /// killed.
    #[serde(default = "default_exec_interval_secs")]
    pub exec_interval_secs: f64,

    /// The maximum number of events to buffer before the oldest events are
    /// dropped.
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
}

impl Default for ScheduledConfig {
    fn default() -> Self {
        Self {
            exec_interval_secs: default_exec_interval_secs(),
            buffer_size: default_buffer_size(),
        }
    }
}

/// Configuration options for streaming commands.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct StreamingConfig {
    /// Whether or not the command should be rerun if the command exits.
    #[serde(default = "default_respawn_on_exit")]
    pub respawn_on_exit: bool,

    /// The amount of time, in seconds, before rerunning a streaming command that exited.
    #[serde(default = "default_respawn_interval_secs")]
    pub respawn_interval_secs: u64,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            respawn_on_exit: default_respawn_on_exit(),
            respawn_interval_secs: default_respawn_interval_secs(),
        }
    }
}

impl StreamingConfig {
    pub(super) const fn respawn_interval(&self) -> Duration {
        Duration::from_secs(self.respawn_interval_secs)
    }
}

/// Configuration options for per-event commands.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct PerEventConfig {
    /// The maximum number of concurrent processes to run.
    ///
    /// - A value of `0` is an invalid configuration.
    /// - A value of `1` will process events in order.
    /// - A value greater than `1` processes events concurrently and unordered.
    #[serde(default = "default_max_concurrent_processes")]
    pub max_concurrent_processes: usize,
}

impl Default for PerEventConfig {
    fn default() -> Self {
        Self {
            max_concurrent_processes: default_max_concurrent_processes(),
        }
    }
}

/// Configuration needed to spawn a process.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct CommandConfig {
    /// The command to run, plus any arguments required.
    #[configurable(metadata(docs::examples = "cat"))]
    pub command: Vec<String>,

    /// Custom environment variables to set or update when running the command.
    /// If a variable name already exists in the environment, its value is replaced.
    #[serde(default)]
    #[configurable(metadata(docs::additional_props_description = "An environment variable."))]
    #[configurable(metadata(docs::examples = "environment_examples()"))]
    pub environment: Option<HashMap<String, String>>,

    /// Whether or not to clear the environment before setting custom environment variables.
    #[serde(default = "default_clear_environment")]
    pub clear_environment: bool,

    /// The directory in which to run the command.
    pub working_directory: Option<PathBuf>,
}

impl Default for CommandConfig {
    fn default() -> Self {
        Self {
            command: vec!["cat".to_owned()],
            environment: None,
            clear_environment: default_clear_environment(),
            working_directory: None,
        }
    }
}

/// Configuration for stdin of the child process.
#[configurable_component]
#[derive(Clone, Debug)]
pub(super) struct StdinConfig {
    /// The encoding configuration to use for encoding events to stdin.
    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,
}

impl Default for StdinConfig {
    fn default() -> Self {
        Self {
            encoding: EncodingConfigWithFraming::new(
                None,
                JsonSerializerConfig::default().into(),
                Transformer::default(),
            ),
        }
    }
}

/// Configuration for stdout of the child process.
#[configurable_component]
#[derive(Clone, Debug)]
pub(super) struct StdoutConfig {
    /// The framing to use for encoding events to stdout.
    #[configurable(derived)]
    #[serde(default)]
    pub framing: Option<decoding::FramingConfig>,

    /// The codec to use for decoding events from stdout.
    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    #[configurable(metadata(docs::hidden))]
    pub log_namespace: Option<bool>,
}

impl Default for StdoutConfig {
    fn default() -> Self {
        Self {
            framing: None,
            decoding: default_decoding(),
            log_namespace: None,
        }
    }
}

/// Configuration for stderr of the child process.
#[configurable_component]
#[derive(Clone, Debug)]
pub(super) struct StderrConfig {
    #[configurable(derived)]
    pub mode: StderrMode,

    #[configurable(derived)]
    #[serde(default)]
    pub framing: Option<decoding::FramingConfig>,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    #[configurable(metadata(docs::hidden))]
    pub log_namespace: Option<bool>,
}

impl Default for StderrConfig {
    fn default() -> Self {
        Self {
            mode: StderrMode::Forward,
            framing: None,
            decoding: default_decoding(),
            log_namespace: None,
        }
    }
}

/// How to handle stderr output from the command.
#[configurable_component]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum StderrMode {
    /// Forward stderr output into the event stream. The event will have the
    /// `stream` metadata field set to `stderr`.
    Forward,

    /// Discard all stderr output from the command.
    Drop,
}

/// Mode of operation for running the command.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Mode {
    /// The command runs continuously, receiving events over stdin and producing
    /// events over stdout.
    ///
    /// The command can optionally be restarted automatically if it exits.
    #[default]
    Streaming,

    /// The command is run on a schedule, receiving a batch of events over stdin
    /// and producing zero or more events over stdout. It then shuts down and
    /// waits for the timer to elapse before running again.
    Scheduled,

    /// The command is started and stopped for each individual event, receiving
    /// a single event over stdin and producing zero or more events over stdout.
    ///
    /// WARNING: This mode spawns a new process for each event. This is not
    /// recommended for high-throughput use cases.
    PerEvent,
}

const fn default_exec_interval_secs() -> f64 {
    60.
}

const fn default_respawn_interval_secs() -> u64 {
    5
}

const fn default_respawn_on_exit() -> bool {
    true
}

const fn default_clear_environment() -> bool {
    false
}

const fn default_buffer_size() -> usize {
    1000
}

const fn default_max_concurrent_processes() -> usize {
    50
}

fn environment_examples() -> HashMap<String, String> {
    HashMap::<_, _>::from_iter([
        ("LANG".to_owned(), "es_ES.UTF-8".to_owned()),
        ("TZ".to_owned(), "Etc/UTC".to_owned()),
        ("PATH".to_owned(), "/bin:/usr/bin:/usr/local/bin".to_owned()),
    ])
}
