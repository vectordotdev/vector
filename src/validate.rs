use crate::{
    config_paths, event,
    runtime::Runtime,
    topology::{self, builder::Pieces, Config, ConfigDiff},
};
use colored::*;
use exitcode::ExitCode;
use futures::compat::Future01CompatExt;
use std::{fmt, fs::File, path::PathBuf};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Disables topology check
    #[structopt(long)]
    no_topology: bool,

    /// Disables environment checks. That includes component checks and health checks.
    #[structopt(long)]
    no_environment: bool,

    /// Shorthand for `--no-topology` and `--no-environment` flags. Just `-n` won't disable anything,
    /// it needs to be used with `t` for `--no-topology`, and or `e` for `--no-environment` in any order.
    /// Example:
    /// `-nte` and `-net` both mean `--no-topology` and `--no-environment`
    #[structopt(short, parse(from_str = NoCheck::from_str), possible_values = &["","t", "e","et","te"], default_value="")]
    no: NoCheck,

    /// Fail validation on warnings
    #[structopt(short, long)]
    deny_warnings: bool,

    /// Any number of Vector config files to validate. If none are specified the
    /// default config path `/etc/vector/vector.toml` will be targeted.
    paths: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug)]
struct NoCheck {
    topology: bool,
    environment: bool,
}

impl NoCheck {
    fn from_str(s: &str) -> Self {
        Self {
            topology: s.find('t').is_some(),
            environment: s.find('e').is_some(),
        }
    }
}

/// Performs topology, component, and health checks.
pub fn validate(opts: &Opts, color: bool) -> ExitCode {
    let mut fmt = Formatter::new(color);

    let mut validated = true;

    let config = match validate_config(opts, &mut fmt) {
        Ok(config) => config,
        Err(Some(config)) => {
            validated &= false;
            config
        }
        Err(None) => return exitcode::CONFIG,
    };

    if !(opts.no_topology || opts.no.topology) {
        validated &= validate_topology(opts, &config, &mut fmt);
    }

    if !(opts.no_environment || opts.no.environment) {
        validated &= validate_environment(&config, &mut fmt);
    }

    if validated {
        fmt.validated();
        exitcode::OK
    } else {
        exitcode::CONFIG
    }
}

/// Ok if all configs were succesfully validated.
/// Err Some contains only succesfully validated configs.
fn validate_config(opts: &Opts, fmt: &mut Formatter) -> Result<Config, Option<Config>> {
    // Prepare paths
    let paths = if let Some(paths) = config_paths::prepare(opts.paths.clone()) {
        paths
    } else {
        fmt.error("No config file paths");
        return Err(None);
    };

    // Validate configuration files
    let to_valdiate = paths.len();
    let mut validated = 0;
    let mut full_config = Config::empty();
    for config_path in paths {
        let file = match File::open(&config_path) {
            Ok(file) => file,
            Err(error) => {
                if let std::io::ErrorKind::NotFound = error.kind() {
                    fmt.error(format!("File {:?} not found", config_path));
                } else {
                    fmt.error(format!(
                        "Failed opening file {:?} with error {:?}",
                        config_path, error
                    ));
                }
                continue;
            }
        };

        trace!(
            message = "Parsing config.",
            path = ?config_path
        );

        let mut sub_failed = |title: String, errors| {
            fmt.title(title);
            fmt.sub_error(errors);
        };

        let mut config = match Config::load(file) {
            Ok(config) => config,
            Err(errors) => {
                sub_failed(format!("Failed to parse {:?}", config_path), errors);
                continue;
            }
        };

        if let Err(errors) = config.expand_macros() {
            sub_failed(
                format!("Failed to expand macros in {:?}", config_path),
                errors,
            );
            continue;
        }

        if let Err(errors) = full_config.append(config) {
            sub_failed(format!("Failed to merge config {:?}", config_path), errors);
            continue;
        }

        validated += 1;
        fmt.success(format!("Loaded {:?}", &config_path));
    }

    if to_valdiate == validated {
        Ok(full_config)
    } else {
        if validated > 0 {
            Err(Some(full_config))
        } else {
            Err(None)
        }
    }
}

fn validate_topology(opts: &Opts, config: &Config, fmt: &mut Formatter) -> bool {
    match topology::builder::check(config) {
        Ok(warnings) => {
            if warnings.is_empty() {
                fmt.success("Configuration topology");
                true
            } else {
                if opts.deny_warnings {
                    fmt.title("Topology errors");
                    fmt.sub_error(warnings);
                    false
                } else {
                    fmt.title("Topology warnings");
                    fmt.sub_warning(warnings);
                    fmt.success("Configuration topology");
                    true
                }
            }
        }
        Err(errors) => {
            fmt.title("Topology errors");
            fmt.sub_error(errors);
            false
        }
    }
}

fn validate_environment(config: &Config, fmt: &mut Formatter) -> bool {
    let mut rt = Runtime::with_thread_count(1).expect("Unable to create async runtime");
    let diff = ConfigDiff::initial(config);

    let mut pieces = if let Some(pieces) = validate_components(config, &diff, &mut rt, fmt) {
        pieces
    } else {
        return false;
    };

    validate_healthchecks(config, &diff, &mut pieces, &mut rt, fmt)
}

fn validate_components(
    config: &Config,
    diff: &ConfigDiff,
    rt: &mut Runtime,
    fmt: &mut Formatter,
) -> Option<Pieces> {
    event::LOG_SCHEMA
        .set(config.global.log_schema.clone())
        .expect("Couldn't set schema");

    match topology::builder::build_pieces(config, diff, rt.executor()) {
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

fn validate_healthchecks(
    config: &Config,
    diff: &ConfigDiff,
    pieces: &mut Pieces,
    rt: &mut Runtime,
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

        let handle = rt.spawn_handle_std(healthcheck.compat());
        match rt.block_on_std(handle) {
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

    /// Final confirmation that validation process was succesfull.
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

    /// A list of warnings that go with a title.
    fn sub_warning<I: IntoIterator>(&mut self, warnings: I)
    where
        I::Item: fmt::Display,
    {
        self.sub(self.warning_intro.clone(), warnings)
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
