#![allow(missing_docs)]

use std::sync::atomic::Ordering;
use std::{num::NonZeroU64, path::PathBuf};

use clap::{ArgAction, CommandFactory, FromArgMatches, Parser};

#[cfg(windows)]
use crate::service;
#[cfg(feature = "api-client")]
use crate::tap;
#[cfg(feature = "api-client")]
use crate::top;
use crate::{config, convert_config, generate, get_version, graph, list, unit_test, validate};
use crate::{generate_schema, signal};

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    #[command(flatten)]
    pub root: RootOpts,

    #[command(subcommand)]
    pub sub_command: Option<SubCommand>,
}

impl Opts {
    pub fn get_matches() -> Result<Self, clap::Error> {
        let version = get_version();
        let app = Opts::command().version(version);
        Opts::from_arg_matches(&app.get_matches())
    }

    pub const fn log_level(&self) -> &'static str {
        let (quiet_level, verbose_level) = match self.sub_command {
            Some(SubCommand::Validate(_))
            | Some(SubCommand::Graph(_))
            | Some(SubCommand::Generate(_))
            | Some(SubCommand::ConvertConfig(_))
            | Some(SubCommand::List(_))
            | Some(SubCommand::Test(_)) => {
                if self.root.verbose == 0 {
                    (self.root.quiet + 1, self.root.verbose)
                } else {
                    (self.root.quiet, self.root.verbose - 1)
                }
            }
            _ => (self.root.quiet, self.root.verbose),
        };
        match quiet_level {
            0 => match verbose_level {
                0 => "info",
                1 => "debug",
                2..=255 => "trace",
            },
            1 => "warn",
            2 => "error",
            3..=255 => "off",
        }
    }
}

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
pub struct RootOpts {
    /// Read configuration from one or more files. Wildcard paths are supported.
    /// File format is detected from the file name.
    /// If zero files are specified, the deprecated default config path
    /// `/etc/vector/vector.yaml` is targeted.
    #[arg(
        id = "config",
        short,
        long,
        env = "VECTOR_CONFIG",
        value_delimiter(',')
    )]
    pub config_paths: Vec<PathBuf>,

    /// Read configuration from files in one or more directories.
    /// File format is detected from the file name.
    ///
    /// Files not ending in .toml, .json, .yaml, or .yml will be ignored.
    #[arg(
        id = "config-dir",
        short = 'C',
        long,
        env = "VECTOR_CONFIG_DIR",
        value_delimiter(',')
    )]
    pub config_dirs: Vec<PathBuf>,

    /// Read configuration from one or more files. Wildcard paths are supported.
    /// TOML file format is expected.
    #[arg(
        id = "config-toml",
        long,
        env = "VECTOR_CONFIG_TOML",
        value_delimiter(',')
    )]
    pub config_paths_toml: Vec<PathBuf>,

    /// Read configuration from one or more files. Wildcard paths are supported.
    /// JSON file format is expected.
    #[arg(
        id = "config-json",
        long,
        env = "VECTOR_CONFIG_JSON",
        value_delimiter(',')
    )]
    pub config_paths_json: Vec<PathBuf>,

    /// Read configuration from one or more files. Wildcard paths are supported.
    /// YAML file format is expected.
    #[arg(
        id = "config-yaml",
        long,
        env = "VECTOR_CONFIG_YAML",
        value_delimiter(',')
    )]
    pub config_paths_yaml: Vec<PathBuf>,

    /// Exit on startup if any sinks fail healthchecks
    #[arg(short, long, env = "VECTOR_REQUIRE_HEALTHY")]
    pub require_healthy: Option<bool>,

    /// Number of threads to use for processing (default is number of available cores)
    #[arg(short, long, env = "VECTOR_THREADS")]
    pub threads: Option<usize>,

    /// Enable more detailed internal logging. Repeat to increase level. Overridden by `--quiet`.
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,

    /// Reduce detail of internal logging. Repeat to reduce further. Overrides `--verbose`.
    #[arg(short, long, action = ArgAction::Count)]
    pub quiet: u8,

    /// Set the logging format
    #[arg(long, default_value = "text", env = "VECTOR_LOG_FORMAT")]
    pub log_format: LogFormat,

    /// Control when ANSI terminal formatting is used.
    ///
    /// By default `vector` will try and detect if `stdout` is a terminal, if it is
    /// ANSI will be enabled. Otherwise it will be disabled. By providing this flag with
    /// the `--color always` option will always enable ANSI terminal formatting. `--color never`
    /// will disable all ANSI terminal formatting. `--color auto` will attempt
    /// to detect it automatically.
    #[arg(long, default_value = "auto", env = "VECTOR_COLOR")]
    pub color: Color,

    /// Watch for changes in configuration file, and reload accordingly.
    #[arg(short, long, env = "VECTOR_WATCH_CONFIG")]
    pub watch_config: bool,

    /// Set the internal log rate limit
    #[arg(
        short,
        long,
        env = "VECTOR_INTERNAL_LOG_RATE_LIMIT",
        default_value = "10"
    )]
    pub internal_log_rate_limit: u64,

    /// Set the duration in seconds to wait for graceful shutdown after SIGINT or SIGTERM are
    /// received. After the duration has passed, Vector will force shutdown. To never force
    /// shutdown, use `--no-graceful-shutdown-limit`.
    #[arg(
        long,
        default_value = "60",
        env = "VECTOR_GRACEFUL_SHUTDOWN_LIMIT_SECS",
        group = "graceful-shutdown-limit"
    )]
    pub graceful_shutdown_limit_secs: NonZeroU64,

    /// Never time out while waiting for graceful shutdown after SIGINT or SIGTERM received.
    /// This is useful when you would like for Vector to attempt to send data until terminated
    /// by a SIGKILL. Overrides/cannot be set with `--graceful-shutdown-limit-secs`.
    #[arg(
        long,
        default_value = "false",
        env = "VECTOR_NO_GRACEFUL_SHUTDOWN_LIMIT",
        group = "graceful-shutdown-limit"
    )]
    pub no_graceful_shutdown_limit: bool,

    /// Set runtime allocation tracing
    #[cfg(feature = "allocation-tracing")]
    #[arg(long, env = "ALLOCATION_TRACING", default_value = "false")]
    pub allocation_tracing: bool,

    /// Set allocation tracing reporting rate in milliseconds.
    #[cfg(feature = "allocation-tracing")]
    #[arg(
        long,
        env = "ALLOCATION_TRACING_REPORTING_INTERVAL_MS",
        default_value = "5000"
    )]
    pub allocation_tracing_reporting_interval_ms: u64,

    /// Disable probing and configuration of root certificate locations on the system for OpenSSL.
    ///
    /// The probe functionality manipulates the `SSL_CERT_FILE` and `SSL_CERT_DIR` environment variables
    /// in the Vector process. This behavior can be problematic for users of the `exec` source, which by
    /// default inherits the environment of the Vector process.
    #[arg(long, env = "VECTOR_OPENSSL_NO_PROBE", default_value = "false")]
    pub openssl_no_probe: bool,

    /// Allow the configuration to run without any components. This is useful for loading in an
    /// empty stub config that will later be replaced with actual components. Note that this is
    /// likely not useful without also watching for config file changes as described in
    /// `--watch-config`.
    #[arg(long, env = "VECTOR_ALLOW_EMPTY_CONFIG", default_value = "false")]
    pub allow_empty_config: bool,

    /// Turn on strict mode for environment variable interpolation. When set, interpolation of a
    /// missing environment variable in configuration files will cause an error instead of a
    /// warning, which will result in a failure to load any such configuration file. This defaults
    /// to false, but that default is deprecated and will be changed to strict in future versions.
    #[arg(long, env = "VECTOR_STRICT_ENV_VARS", default_value = "false")]
    pub strict_env_vars: bool,
}

impl RootOpts {
    /// Return a list of config paths with the associated formats.
    pub fn config_paths_with_formats(&self) -> Vec<config::ConfigPath> {
        config::merge_path_lists(vec![
            (&self.config_paths, None),
            (&self.config_paths_toml, Some(config::Format::Toml)),
            (&self.config_paths_json, Some(config::Format::Json)),
            (&self.config_paths_yaml, Some(config::Format::Yaml)),
        ])
        .map(|(path, hint)| config::ConfigPath::File(path, hint))
        .chain(
            self.config_dirs
                .iter()
                .map(|dir| config::ConfigPath::Dir(dir.to_path_buf())),
        )
        .collect()
    }

    pub fn init_global(&self) {
        crate::config::STRICT_ENV_VARS.store(self.strict_env_vars, Ordering::Relaxed);

        if !self.openssl_no_probe {
            openssl_probe::init_ssl_cert_env_vars();
        }

        #[cfg(not(feature = "enterprise-tests"))]
        crate::metrics::init_global().expect("metrics initialization failed");
    }
}

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
pub enum SubCommand {
    /// Validate the target config, then exit.
    Validate(validate::Opts),

    /// Convert a config file from one format to another.
    /// This command can also walk directories recursively and convert all config files that are discovered.
    /// Note that this is a best effort conversion due to the following reasons:
    /// * The comments from the original config file are not preserved.
    /// * Explicitly set default values in the original implementation might be omitted.
    /// * Depending on how each source/sink config struct configures serde, there might be entries with null values.
    ConvertConfig(convert_config::Opts),

    /// Generate a Vector configuration containing a list of components.
    Generate(generate::Opts),

    /// Generate the configuration schema for this version of Vector. (experimental)
    ///
    /// A JSON Schema document will be written to stdout that represents the valid schema for a
    /// Vector configuration. This schema is based on the "full" configuration, such that for usages
    /// where a configuration is split into multiple files, the schema would apply to those files
    /// only when concatenated together.
    GenerateSchema,

    /// Output a provided Vector configuration file/dir as a single JSON object, useful for checking in to version control.
    #[command(hide = true)]
    Config(config::Opts),

    /// List available components, then exit.
    List(list::Opts),

    /// Run Vector config unit tests, then exit. This command is experimental and therefore subject to change.
    /// For guidance on how to write unit tests check out <https://vector.dev/guides/level-up/unit-testing/>.
    Test(unit_test::Opts),

    /// Output the topology as visual representation using the DOT language which can be rendered by GraphViz
    Graph(graph::Opts),

    /// Display topology and metrics in the console, for a local or remote Vector instance
    #[cfg(feature = "api-client")]
    Top(top::Opts),

    /// Observe output log events from source or transform components. Logs are sampled at a specified interval.
    #[cfg(feature = "api-client")]
    Tap(tap::Opts),

    /// Manage the vector service.
    #[cfg(windows)]
    Service(service::Opts),

    /// Vector Remap Language CLI
    Vrl(vrl::cli::Opts),
}

impl SubCommand {
    pub async fn execute(
        &self,
        mut signals: signal::SignalPair,
        color: bool,
    ) -> exitcode::ExitCode {
        match self {
            Self::Config(c) => config::cmd(c),
            Self::ConvertConfig(opts) => convert_config::cmd(opts),
            Self::Generate(g) => generate::cmd(g),
            Self::GenerateSchema => generate_schema::cmd(),
            Self::Graph(g) => graph::cmd(g),
            Self::List(l) => list::cmd(l),
            #[cfg(windows)]
            Self::Service(s) => service::cmd(s),
            #[cfg(feature = "api-client")]
            Self::Tap(t) => tap::cmd(t, signals.receiver).await,
            Self::Test(t) => unit_test::cmd(t, &mut signals.handler).await,
            #[cfg(feature = "api-client")]
            Self::Top(t) => top::cmd(t).await,
            Self::Validate(v) => validate::validate(v, color).await,
            Self::Vrl(s) => {
                let mut functions = vrl::stdlib::all();
                functions.extend(vector_vrl_functions::all());
                vrl::cli::cmd::cmd(s, functions)
            }
        }
    }
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Auto,
    Always,
    Never,
}

impl Color {
    pub fn use_color(&self) -> bool {
        match self {
            #[cfg(unix)]
            Color::Auto => {
                use std::io::IsTerminal;
                std::io::stdout().is_terminal()
            }
            #[cfg(windows)]
            Color::Auto => false, // ANSI colors are not supported by cmd.exe
            Color::Always => true,
            Color::Never => false,
        }
    }
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Text,
    Json,
}

pub fn handle_config_errors(errors: Vec<String>) -> exitcode::ExitCode {
    for error in errors {
        error!(message = "Configuration error.", %error);
    }

    exitcode::CONFIG
}
