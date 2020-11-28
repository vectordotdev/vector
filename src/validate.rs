use crate::{
    config::{self, Config, ConfigDiff},
    topology::{self, builder::Pieces},
};
use colored::*;
use exitcode::ExitCode;
use std::{fmt, fs::remove_dir_all, path::PathBuf};
use structopt::StructOpt;

const TEMPORARY_DIRECTORY: &str = "validate_tmp";

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Disables environment checks. That includes component checks and health checks.
    #[structopt(long)]
    no_environment: bool,

    /// Fail validation on warnings that are probably a mistake in the configuration
    /// or are recommended to be fixed.
    #[structopt(short, long)]
    deny_warnings: bool,

    /// Vector config files in TOML format to validate.
    #[structopt(name = "config-toml", long)]
    paths_toml: Vec<PathBuf>,

    /// Vector config files in JSON format to validate.
    #[structopt(name = "config-json", long)]
    paths_json: Vec<PathBuf>,

    /// Vector config files in YAML format to validate.
    #[structopt(name = "config-yaml", long)]
    paths_yaml: Vec<PathBuf>,

    /// Any number of Vector config files to validate.
    /// Format is detected from the file name.
    /// If none are specified the default config path `/etc/vector/vector.toml`
    /// will be targeted.
    paths: Vec<PathBuf>,
}

impl Opts {
    fn paths_with_formats(&self) -> Vec<(PathBuf, config::FormatHint)> {
        config::merge_path_lists(vec![
            (&self.paths, None),
            (&self.paths_toml, Some(config::Format::TOML)),
            (&self.paths_json, Some(config::Format::JSON)),
            (&self.paths_yaml, Some(config::Format::YAML)),
        ])
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

/// Ok if all configs were successfully validated.
/// Err Some contains only successfully validated configs.
fn validate_config(opts: &Opts, fmt: &mut Formatter) -> Option<Config> {
    // Prepare paths
    let paths = opts.paths_with_formats();
    let paths = if let Some(paths) = config::process_paths(&paths) {
        paths
    } else {
        fmt.error("No config file paths");
        return None;
    };

    let paths_list: Vec<_> = paths.iter().map(|(path, _)| path).collect();
    match config::load_from_paths(&paths, opts.deny_warnings) {
        Ok(config) => {
            fmt.success(format!("Loaded {:?}", &paths_list));
            Some(config)
        }
        Err(errors) => {
            fmt.title(format!("Failed to load {:?}", &paths_list));
            fmt.sub_error(errors);
            None
        }
    }
}

async fn validate_environment(opts: &Opts, config: &Config, fmt: &mut Formatter) -> bool {
    let diff = ConfigDiff::initial(config);

    let mut pieces = if let Some(pieces) = validate_components(config, &diff, fmt).await {
        pieces
    } else {
        return false;
    };

    validate_healthchecks(opts, config, &diff, &mut pieces, fmt).await
}

async fn validate_components(
    config: &Config,
    diff: &ConfigDiff,
    fmt: &mut Formatter,
) -> Option<Pieces> {
    crate::config::LOG_SCHEMA
        .set(config.global.log_schema.clone())
        .expect("Couldn't set schema");

    match topology::builder::build_pieces(config, diff).await {
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
    pieces: &mut Pieces,
    fmt: &mut Formatter,
) -> bool {
    let healthchecks = topology::take_healthchecks(diff, pieces);
    // We are running health checks in serial so it's easier for the users
    // to parse which errors/warnings/etc. belong to which healthcheck.
    let mut validated = true;
    for (name, healthcheck) in healthchecks {
        let mut failed = |error| {
            validated = false;
            fmt.error(error);
        };

        match tokio::spawn(healthcheck).await {
            Ok(Ok(())) => {
                if config
                    .sinks
                    .get(&name)
                    .expect("Sink not present")
                    .healthcheck
                {
                    fmt.success(format!("Health check `{}`", name.as_str()));
                } else {
                    fmt.warning(format!("Health check disabled for `{}`", name));
                    validated &= !opts.deny_warnings;
                }
            }
            Ok(Err(())) => failed(format!("Health check for `{}` failed", name.as_str())),
            Err(error) if error.is_cancelled() => failed(format!(
                "Health check for `{}` was cancelled",
                name.as_str()
            )),
            Err(_) => failed(format!("Health check for `{}` panicked", name.as_str())),
        }
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
            fmt.error(format!("{}", error));
            None
        }
    }
}

fn remove_tmp_directory(path: PathBuf) {
    if let Err(error) = remove_dir_all(&path) {
        error!(message = "Failed to remove temporary directory.", path = ?path, %error);
    }
}

struct Formatter {
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
    fn new(color: bool) -> Self {
        Self {
            max_line_width: 0,
            print_space: false,
            error_intro: if color {
                format!("{}", "x".red())
            } else {
                "x".to_owned()
            },
            warning_intro: if color {
                format!("{}", "~".yellow())
            } else {
                "~".to_owned()
            },
            success_intro: if color {
                format!("{}", "√".green())
            } else {
                "√".to_owned()
            },
            color,
        }
    }

    /// Final confirmation that validation process was successful.
    fn validated(&self) {
        println!("{:-^width$}", "", width = self.max_line_width);
        if self.color {
            // Coloring needs to be used directly so that print
            // infrastructure correctly determines length of the
            // "Validated". Otherwise, ansi escape coloring is
            // calculated into the length.
            println!(
                "{:>width$}",
                "Validated".green(),
                width = self.max_line_width
            );
        } else {
            println!("{:>width$}", "Validated", width = self.max_line_width)
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
            println!();
        }
    }

    fn print(&mut self, print: impl AsRef<str>) {
        let width = print
            .as_ref()
            .lines()
            .map(|line| {
                String::from_utf8_lossy(&strip_ansi_escapes::strip(line).unwrap())
                    .chars()
                    .count()
            })
            .max()
            .unwrap_or(0);
        self.max_line_width = width.max(self.max_line_width);
        self.print_space = true;
        print!("{}", print.as_ref())
    }
}
