use std::path::PathBuf;

use structopt::{clap::AppSettings, StructOpt};

#[cfg(windows)]
use crate::service;
#[cfg(feature = "api-client")]
use crate::tap;
#[cfg(feature = "api-client")]
use crate::top;
use crate::{config, generate, get_version, graph, list, unit_test, validate};

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    #[structopt(flatten)]
    pub root: RootOpts,

    #[structopt(subcommand)]
    pub sub_command: Option<SubCommand>,
}

impl Opts {
    pub fn get_matches() -> Self {
        let version = get_version();
        let app = Opts::clap().version(version.as_str()).global_settings(&[
            AppSettings::ColoredHelp,
            AppSettings::InferSubcommands,
            AppSettings::DeriveDisplayOrder,
        ]);
        Opts::from_clap(&app.get_matches())
    }

    pub const fn log_level(&self) -> &'static str {
        let (quiet_level, verbose_level) = match self.sub_command {
            Some(SubCommand::Validate(_))
            | Some(SubCommand::Graph(_))
            | Some(SubCommand::Generate(_))
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

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct RootOpts {
    /// Read configuration from one or more files. Wildcard paths are supported.
    /// File format is detected from the file name.
    /// If zero files are specified the default config path
    /// `/etc/vector/vector.toml` will be targeted.
    #[structopt(
        name = "config",
        short,
        long,
        env = "VECTOR_CONFIG",
        use_delimiter(true)
    )]
    pub config_paths: Vec<PathBuf>,

    /// Read configuration from files in one or more directories.
    /// File format is detected from the file name.
    ///
    /// Files not ending in .toml, .json, .yaml, or .yml will be ignored.
    #[structopt(
        name = "config-dir",
        short = "C",
        long,
        env = "VECTOR_CONFIG_DIR",
        use_delimiter(true)
    )]
    pub config_dirs: Vec<PathBuf>,

    /// Read configuration from one or more files. Wildcard paths are supported.
    /// TOML file format is expected.
    #[structopt(
        name = "config-toml",
        long,
        env = "VECTOR_CONFIG_TOML",
        use_delimiter(true)
    )]
    pub config_paths_toml: Vec<PathBuf>,

    /// Read configuration from one or more files. Wildcard paths are supported.
    /// JSON file format is expected.
    #[structopt(
        name = "config-json",
        long,
        env = "VECTOR_CONFIG_JSON",
        use_delimiter(true)
    )]
    pub config_paths_json: Vec<PathBuf>,

    /// Read configuration from one or more files. Wildcard paths are supported.
    /// YAML file format is expected.
    #[structopt(
        name = "config-yaml",
        long,
        env = "VECTOR_CONFIG_YAML",
        use_delimiter(true)
    )]
    pub config_paths_yaml: Vec<PathBuf>,

    /// Exit on startup if any sinks fail healthchecks
    #[structopt(short, long, env = "VECTOR_REQUIRE_HEALTHY")]
    pub require_healthy: Option<bool>,

    /// Number of threads to use for processing (default is number of available cores)
    #[structopt(short, long, env = "VECTOR_THREADS")]
    pub threads: Option<usize>,

    /// Enable more detailed internal logging. Repeat to increase level. Overridden by `--quiet`.
    #[structopt(short, long, parse(from_occurrences))]
    pub verbose: u8,

    /// Reduce detail of internal logging. Repeat to reduce further. Overrides `--verbose`.
    #[structopt(short, long, parse(from_occurrences))]
    pub quiet: u8,

    /// Set the logging format
    #[structopt(long, default_value = "text", env = "VECTOR_LOG_FORMAT", possible_values = &["text", "json"])]
    pub log_format: LogFormat,

    /// Control when ANSI terminal formatting is used.
    ///
    /// By default `vector` will try and detect if `stdout` is a terminal, if it is
    /// ANSI will be enabled. Otherwise it will be disabled. By providing this flag with
    /// the `--color always` option will always enable ANSI terminal formatting. `--color never`
    /// will disable all ANSI terminal formatting. `--color auto` will attempt
    /// to detect it automatically.
    #[structopt(long, default_value = "auto", env = "VECTOR_COLOR", possible_values = &["auto", "always", "never"])]
    pub color: Color,

    /// Watch for changes in configuration file, and reload accordingly.
    #[structopt(short, long, env = "VECTOR_WATCH_CONFIG")]
    pub watch_config: bool,
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
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub enum SubCommand {
    /// Validate the target config, then exit.
    Validate(validate::Opts),

    /// Generate a Vector configuration containing a list of components.
    Generate(generate::Opts),

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
    #[cfg(feature = "vrl-cli")]
    Vrl(vrl_cli::Opts),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Color {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogFormat {
    Text,
    Json,
}

impl std::str::FromStr for Color {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(Color::Auto),
            "always" => Ok(Color::Always),
            "never" => Ok(Color::Never),
            s => Err(format!(
                "{} is not a valid option, expected `auto`, `always` or `never`",
                s
            )),
        }
    }
}

impl std::str::FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(LogFormat::Text),
            "json" => Ok(LogFormat::Json),
            s => Err(format!(
                "{} is not a valid option, expected `text` or `json`",
                s
            )),
        }
    }
}

pub fn handle_config_errors(errors: Vec<String>) -> exitcode::ExitCode {
    for error in errors {
        error!(message = "Configuration error.", %error);
    }

    exitcode::CONFIG
}
