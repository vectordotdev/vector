use crate::{
    config_paths, event,
    runtime::Runtime,
    topology::{self, Config, ConfigDiff},
};
use std::{fs::File, path::PathBuf};
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

    /// Shorthand for `--no_topology` and `--no_environment` flags. Just `-n` won't disable anything,
    /// it needs to be used with `t` for `--no_topology`, and or `e` for `--no_environment` in any order.
    /// Example:
    /// `-nte` and `-net` both mean `--no_topology` and `--no_environment`
    #[structopt(short, parse(from_str = NoCheck::from_str), possible_values = &["t", "e","et","te"], default_value)]
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

impl Default for NoCheck {
    fn default() -> Self {
        NoCheck {
            topology: false,
            environment: false,
        }
    }
}

impl std::fmt::Display for NoCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.topology {
            write!(f, "`--no_topology` ")?;
        }
        if self.environment {
            write!(f, "`--no_environment`")?;
        }
        Ok(())
    }
}

/// Performs topology, component, and health checks.
pub fn validate(opts: &Opts, color: bool) -> exitcode::ExitCode {
    use colored::*;
    use futures::compat::Future01CompatExt;

    // Print constants,functions
    let max_line_width = std::cell::Cell::new(0);
    let print_space = std::cell::Cell::new(false);
    let print = |print: String| {
        max_line_width.set(
            print
                .lines()
                .map(|line| {
                    String::from_utf8_lossy(&strip_ansi_escapes::strip(line).unwrap())
                        .chars()
                        .count()
                })
                .max()
                .unwrap_or(0)
                .max(max_line_width.get()),
        );
        print_space.set(true);
        print!("{}", print)
    };
    let space = || {
        if print_space.get() {
            print_space.set(false);
            println!();
        }
    };
    let print_sub = |intro, errors| {
        for error in errors {
            print(format!("{} {}\n", intro, error));
        }
        space();
    };

    let print_title = |title: &str| {
        space();
        print(format!("{}\n{:-<width$}\n", title, "", width = title.len()))
    };

    let error_intro = if color {
        format!("{}", "x".red())
    } else {
        "x".to_owned()
    };
    let warning_intro = if color {
        format!("{}", "~".yellow())
    } else {
        "~".to_owned()
    };
    let success_intro = if color {
        format!("{}", "√".green())
    } else {
        "√".to_owned()
    };
    let print_errors = |errors| print_sub(&error_intro, errors);
    let print_error = |error| print(format!("{} {}\n", error_intro, error));
    let print_warning = |warning| print(format!("{} {}\n", warning_intro, warning));
    let print_warnings = |warnings| {
        let intro = if opts.deny_warnings {
            &error_intro
        } else {
            &warning_intro
        };
        print_sub(intro, warnings);
    };
    let print_success = |message: &str| print(format!("{} {}\n", success_intro, message));

    // Prepare paths
    let paths = if let Some(paths) = config_paths::prepare(opts.paths.clone()) {
        paths
    } else {
        print_error("No config file paths".to_owned());
        return exitcode::CONFIG;
    };

    let mut validated = true;

    // Validate configuration files
    let mut success = true;
    let mut full_config = Config::empty();
    for config_path in paths {
        let file = match File::open(&config_path) {
            Ok(file) => file,
            Err(error) => {
                success = false;
                if let std::io::ErrorKind::NotFound = error.kind() {
                    print_error(format!("File {:?} not found", config_path));
                } else {
                    print_error(format!(
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
            success = false;
            print_title(title.as_str());
            print_errors(errors);
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

        print_success(format!("Loaded {:?}", &config_path).as_str());
    }
    validated &= success;

    if !success {
        return exitcode::CONFIG;
    }

    // Validate topology
    if !(opts.no_topology || opts.no.topology) {
        let success = match topology::builder::check(&full_config) {
            Ok(warnings) => {
                if warnings.is_empty() {
                    true
                } else {
                    print_title("Topology warnings");
                    print_warnings(warnings);
                    !opts.deny_warnings
                }
            }
            Err(errors) => {
                print_title("Topology errors");
                print_errors(errors);
                false
            }
        };

        if success {
            print_success("Configuration topology");
        }
        validated &= success;
    }

    // Validate environment
    if !(opts.no_environment || opts.no.environment) {
        // Validate configuration of components
        event::LOG_SCHEMA
            .set(full_config.global.log_schema.clone())
            .expect("Couldn't set schema");

        let mut rt = Runtime::with_thread_count(1).expect("Unable to create async runtime");
        let diff = ConfigDiff::initial(&full_config);
        let mut pieces = match topology::builder::build_pieces(&full_config, &diff, rt.executor()) {
            Ok(pieces) => pieces,
            Err(errors) => {
                print_title("Component errors");
                print_errors(errors);
                return exitcode::CONFIG;
            }
        };
        print_success("Component configuration");

        // Validate health checks
        let healthchecks = topology::take_healthchecks(&diff, &mut pieces);
        // We are running health checks in serial so it's easier for the users
        // to parse which errors/warnings/etc. belong to which healthcheck.
        let mut success = true;
        for (name, healthcheck) in healthchecks {
            let mut failed = |error| {
                success = false;
                print_error(error);
            };

            let handle = rt.spawn_handle(healthcheck.compat());
            match rt.block_on_std(handle) {
                Ok(Ok(())) => {
                    if full_config
                        .sinks
                        .get(&name)
                        .expect("Sink not present")
                        .healthcheck
                    {
                        print_success(format!("Health check `{}`", name.as_str()).as_str());
                    } else {
                        print_warning(format!("Health check disabled for `{}`", name));
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
        validated &= success;
        space();
    }

    if validated {
        println!(
            "{:-^width$}\n{:>width$}",
            "",
            "Validated".green(),
            width = max_line_width.get()
        );
        exitcode::OK
    } else {
        exitcode::CONFIG
    }
}
