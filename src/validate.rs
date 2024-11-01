#![allow(missing_docs)]

use std::{collections::HashMap, fmt, fs::remove_dir_all, path::PathBuf};

use clap::Parser;
use colored::*;
use exitcode::ExitCode;

use crate::{
    config::{self, Config, ConfigDiff},
    extra_context::ExtraContext,
    topology::{self, builder::TopologyPieces},
};

const TEMPORARY_DIRECTORY: &str = "validate_tmp";

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    /// Disables environment checks. That includes component checks and health checks.
    #[arg(long)]
    pub no_environment: bool,

    /// Disables health checks during validation.
    #[arg(long)]
    pub skip_healthchecks: bool,

    /// Fail validation on warnings that are probably a mistake in the configuration
    /// or are recommended to be fixed.
    #[arg(short, long)]
    pub deny_warnings: bool,

    /// Vector config files in TOML format to validate.
    #[arg(
        id = "config-toml",
        long,
        env = "VECTOR_CONFIG_TOML",
        value_delimiter(',')
    )]
    pub paths_toml: Vec<PathBuf>,

    /// Vector config files in JSON format to validate.
    #[arg(
        id = "config-json",
        long,
        env = "VECTOR_CONFIG_JSON",
        value_delimiter(',')
    )]
    pub paths_json: Vec<PathBuf>,

    /// Vector config files in YAML format to validate.
    #[arg(
        id = "config-yaml",
        long,
        env = "VECTOR_CONFIG_YAML",
        value_delimiter(',')
    )]
    pub paths_yaml: Vec<PathBuf>,

    /// Any number of Vector config files to validate.
    /// Format is detected from the file name.
    /// If none are specified, the default config path `/etc/vector/vector.yaml`
    /// is targeted.
    #[arg(env = "VECTOR_CONFIG", value_delimiter(','))]
    pub paths: Vec<PathBuf>,

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
}

impl Opts {
    fn paths_with_formats(&self) -> Vec<config::ConfigPath> {
        config::merge_path_lists(vec![
            (&self.paths, None),
            (&self.paths_toml, Some(config::Format::Toml)),
            (&self.paths_json, Some(config::Format::Json)),
            (&self.paths_yaml, Some(config::Format::Yaml)),
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

/// Performs topology, component, and health checks.
pub async fn validate(opts: &Opts, color: bool) -> ExitCode {
    let mut fmt = Formatter::new(color);

    let mut validated = true;

    let mut config = match validate_config(opts, &mut fmt) {
        Some(config) => config,
        None => return exitcode::CONFIG,
    };

    if !opts.no_environment {
        if let Some(tmp_directory) = create_tmp_directory(&mut config, &mut fmt) {
            validated &= validate_environment(opts, &config, &mut fmt).await;
            remove_tmp_directory(tmp_directory);
        } else {
            validated = false;
        }
    }

    if validated {
        fmt.validated();
        exitcode::OK
    } else {
        exitcode::CONFIG
    }
}

pub fn validate_config(opts: &Opts, fmt: &mut Formatter) -> Option<Config> {
    // Prepare paths
    let paths = opts.paths_with_formats();
    let paths = if let Some(paths) = config::process_paths(&paths) {
        paths
    } else {
        fmt.error("No config file paths");
        return None;
    };

    // Load
    let paths_list: Vec<_> = paths.iter().map(<&PathBuf>::from).collect();

    let mut report_error = |errors| {
        fmt.title(format!("Failed to load {:?}", &paths_list));
        fmt.sub_error(errors);
    };
    let builder = config::load_builder_from_paths(&paths)
        .map_err(&mut report_error)
        .ok()?;
    config::init_log_schema(builder.global.log_schema.clone(), true);

    // Build
    let (config, warnings) = builder
        .build_with_warnings()
        .map_err(&mut report_error)
        .ok()?;

    // Warnings
    if !warnings.is_empty() {
        if opts.deny_warnings {
            report_error(warnings);
            return None;
        }

        fmt.title(format!("Loaded with warnings {:?}", &paths_list));
        fmt.sub_warning(warnings);
    } else {
        fmt.success(format!("Loaded {:?}", &paths_list));
    }

    Some(config)
}

async fn validate_environment(opts: &Opts, config: &Config, fmt: &mut Formatter) -> bool {
    let diff = ConfigDiff::initial(config);

    let mut pieces = if let Some(pieces) = validate_components(config, &diff, fmt).await {
        pieces
    } else {
        return false;
    };
    opts.skip_healthchecks || validate_healthchecks(opts, config, &diff, &mut pieces, fmt).await
}

async fn validate_components(
    config: &Config,
    diff: &ConfigDiff,
    fmt: &mut Formatter,
) -> Option<TopologyPieces> {
    match topology::TopologyPieces::build(config, diff, HashMap::new(), ExtraContext::default())
        .await
    {
        Ok(pieces) => {
            fmt.success("Component configuration");
            Some(pieces)
        }
        Err(errors) => {
            fmt.title("Component errors");
            fmt.sub_error(errors);
            None
        }
    }
}

async fn validate_healthchecks(
    opts: &Opts,
    config: &Config,
    diff: &ConfigDiff,
    pieces: &mut TopologyPieces,
    fmt: &mut Formatter,
) -> bool {
    if !config.healthchecks.enabled {
        fmt.warning("Health checks are disabled");
        return !opts.deny_warnings;
    }

    let healthchecks = topology::take_healthchecks(diff, pieces);
    // We are running health checks in serial so it's easier for the users
    // to parse which errors/warnings/etc. belong to which healthcheck.
    let mut validated = true;
    for (id, healthcheck) in healthchecks {
        let mut failed = |error| {
            validated = false;
            fmt.error(error);
        };

        trace!("Healthcheck for {id} starting.");
        match tokio::spawn(healthcheck).await {
            Ok(Ok(_)) => {
                if config
                    .sink(&id)
                    .expect("Sink not present")
                    .healthcheck()
                    .enabled
                {
                    fmt.success(format!("Health check \"{}\"", id));
                } else {
                    fmt.warning(format!("Health check disabled for \"{}\"", id));
                    validated &= !opts.deny_warnings;
                }
            }
            Ok(Err(e)) => failed(format!("Health check for \"{}\" failed: {}", id, e)),
            Err(error) if error.is_cancelled() => {
                failed(format!("Health check for \"{}\" was cancelled", id))
            }
            Err(_) => failed(format!("Health check for \"{}\" panicked", id)),
        }
        trace!("Healthcheck for {id} done.");
    }

    validated
}

/// For data directory that we write to:
/// 1. Create a tmp directory in it.
/// 2. Change config to point to that tmp directory.
fn create_tmp_directory(config: &mut Config, fmt: &mut Formatter) -> Option<PathBuf> {
    match config
        .global
        .resolve_and_make_data_subdir(None, TEMPORARY_DIRECTORY)
    {
        Ok(path) => {
            config.global.data_dir = Some(path.clone());
            Some(path)
        }
        Err(error) => {
            fmt.error(error.to_string());
            None
        }
    }
}

fn remove_tmp_directory(path: PathBuf) {
    if let Err(error) = remove_dir_all(&path) {
        error!(message = "Failed to remove temporary directory.", path = ?path, %error);
    }
}

pub struct Formatter {
    /// Width of largest printed line
    max_line_width: usize,
    /// Can empty line be printed
    print_space: bool,
    color: bool,
    // Intros
    error_intro: String,
    warning_intro: String,
    success_intro: String,
}

impl Formatter {
    pub fn new(color: bool) -> Self {
        Self {
            max_line_width: 0,
            print_space: false,
            error_intro: if color {
                "x".red().to_string()
            } else {
                "x".to_owned()
            },
            warning_intro: if color {
                "~".yellow().to_string()
            } else {
                "~".to_owned()
            },
            success_intro: if color {
                "√".green().to_string()
            } else {
                "√".to_owned()
            },
            color,
        }
    }

    /// Final confirmation that validation process was successful.
    fn validated(&self) {
        #[allow(clippy::print_stdout)]
        {
            println!("{:-^width$}", "", width = self.max_line_width);
        }
        if self.color {
            // Coloring needs to be used directly so that print
            // infrastructure correctly determines length of the
            // "Validated". Otherwise, ansi escape coloring is
            // calculated into the length.
            #[allow(clippy::print_stdout)]
            {
                println!(
                    "{:>width$}",
                    "Validated".green(),
                    width = self.max_line_width
                );
            }
        } else {
            #[allow(clippy::print_stdout)]
            {
                println!("{:>width$}", "Validated", width = self.max_line_width)
            }
        }
    }

    /// Standalone line
    fn success(&mut self, msg: impl AsRef<str>) {
        self.print(format!("{} {}\n", self.success_intro, msg.as_ref()))
    }

    /// Standalone line
    fn warning(&mut self, warning: impl AsRef<str>) {
        self.print(format!("{} {}\n", self.warning_intro, warning.as_ref()))
    }

    /// Standalone line
    fn error(&mut self, error: impl AsRef<str>) {
        self.print(format!("{} {}\n", self.error_intro, error.as_ref()))
    }

    /// Marks sub
    fn title(&mut self, title: impl AsRef<str>) {
        self.space();
        self.print(format!(
            "{}\n{:-<width$}\n",
            title.as_ref(),
            "",
            width = title.as_ref().len()
        ))
    }

    /// A list of warnings that go with a title.
    fn sub_warning<I: IntoIterator>(&mut self, warnings: I)
    where
        I::Item: fmt::Display,
    {
        self.sub(self.warning_intro.clone(), warnings)
    }

    /// A list of errors that go with a title.
    fn sub_error<I: IntoIterator>(&mut self, errors: I)
    where
        I::Item: fmt::Display,
    {
        self.sub(self.error_intro.clone(), errors)
    }

    fn sub<I: IntoIterator>(&mut self, intro: impl AsRef<str>, msgs: I)
    where
        I::Item: fmt::Display,
    {
        for msg in msgs {
            self.print(format!("{} {}\n", intro.as_ref(), msg));
        }
        self.space();
    }

    /// Prints empty space if necessary.
    fn space(&mut self) {
        if self.print_space {
            self.print_space = false;
            #[allow(clippy::print_stdout)]
            {
                println!();
            }
        }
    }

    fn print(&mut self, print: impl AsRef<str>) {
        let width = print
            .as_ref()
            .lines()
            .map(|line| {
                String::from_utf8_lossy(&strip_ansi_escapes::strip(line))
                    .chars()
                    .count()
            })
            .max()
            .unwrap_or(0);
        self.max_line_width = width.max(self.max_line_width);
        self.print_space = true;
        #[allow(clippy::print_stdout)]
        {
            print!("{}", print.as_ref())
        }
    }
}
