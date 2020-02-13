#[macro_use]
extern crate tracing;

use futures::{future, Future, Stream};
use std::{
    cmp::{max, min},
    fs::File,
    net::SocketAddr,
    path::{Path, PathBuf},
};
use structopt::{clap::AppSettings, StructOpt};
#[cfg(unix)]
use tokio_signal::unix::{Signal, SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use topology::Config;
use tracing_futures::Instrument;
use vector::{config_paths, event, generate, list, metrics, runtime, topology, trace, unit_test};

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
struct Opts {
    #[structopt(flatten)]
    root: RootOpts,

    #[structopt(subcommand)]
    sub_command: Option<SubCommand>,
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
struct RootOpts {
    /// Read configuration from one or more files. Wildcard paths are supported.
    /// If zero files are specified the default config path
    /// `/etc/vector/vector.toml` will be targeted.
    #[structopt(name = "config", short, long)]
    config_paths: Vec<PathBuf>,

    /// Exit on startup if any sinks fail healthchecks
    #[structopt(short, long)]
    require_healthy: bool,

    /// Exit on startup after config verification and optional healthchecks are run
    #[structopt(short, long)]
    dry_run: bool,

    /// Serve internal metrics from the given address
    #[structopt(short, long)]
    metrics_addr: Option<SocketAddr>,

    /// Number of threads to use for processing (default is number of available cores)
    #[structopt(short, long)]
    threads: Option<usize>,

    /// Enable more detailed internal logging. Repeat to increase level. Overridden by `--quiet`.
    #[structopt(short, long, parse(from_occurrences))]
    verbose: u8,

    /// Reduce detail of internal logging. Repeat to reduce further. Overrides `--verbose`.
    #[structopt(short, long, parse(from_occurrences))]
    quiet: u8,

    /// Control when ANSI terminal formatting is used.
    ///
    /// By default `vector` will try and detect if `stdout` is a terminal, if it is
    /// ANSI will be enabled. Otherwise it will be disabled. By providing this flag with
    /// the `--color always` option will always enable ANSI terminal formatting. `--color never`
    /// will disable all ANSI terminal formatting. `--color auto`, the default option, will attempt
    /// to detect it automatically.
    ///
    /// Options: `auto`, `always` or `never`
    #[structopt(long)]
    color: Option<Color>,

    /// Watch for changes in configuration file, and reload accordingly.
    #[structopt(short, long)]
    watch_config: bool,
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
enum SubCommand {
    /// Validate the target config, then exit.
    Validate(Validate),

    /// Generate a Vector configuration containing a list of components.
    Generate(generate::Opts),

    /// List available components, then exit.
    List(list::Opts),

    /// Run Vector config unit tests, then exit. This command is experimental and therefore subject to change.
    /// For guidance on how to write unit tests check out: https://vector.dev/docs/setup/guides/unit-testing/
    Test(unit_test::Opts),
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
struct Validate {
    /// Ensure that the config topology is correct and that all components resolve
    #[structopt(short, long)]
    topology: bool,

    /// Fail validation on warnings
    #[structopt(short, long)]
    deny_warnings: bool,

    /// Any number of Vector config files to validate. If none are specified the
    /// default config path `/etc/vector/vector.toml` will be targeted.
    paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
enum Color {
    Auto,
    Always,
    Never,
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

fn get_version() -> String {
    #[cfg(feature = "nightly")]
    let pkg_version = format!("{}-nightly", built_info::PKG_VERSION);
    #[cfg(not(feature = "nightly"))]
    let pkg_version = built_info::PKG_VERSION;

    let commit_hash = built_info::GIT_VERSION.and_then(|v| v.split('-').last());
    let built_date = chrono::DateTime::parse_from_rfc2822(built_info::BUILT_TIME_UTC)
        .unwrap()
        .format("%Y-%m-%d");
    let built_string = if let Some(commit_hash) = commit_hash {
        format!("{} {} {}", commit_hash, built_info::TARGET, built_date)
    } else {
        built_info::TARGET.into()
    };
    format!("{} ({})", pkg_version, built_string)
}

fn main() {
    openssl_probe::init_ssl_cert_env_vars();
    let version = get_version();
    let app = Opts::clap().version(&version[..]).global_settings(&[
        AppSettings::ColoredHelp,
        AppSettings::InferSubcommands,
        AppSettings::DeriveDisplayOrder,
    ]);
    let root_opts = Opts::from_clap(&app.get_matches());
    let opts = root_opts.root;
    let sub_command = root_opts.sub_command;

    let level = match opts.quiet {
        0 => match opts.verbose {
            0 => "info",
            1 => "debug",
            2..=255 => "trace",
        },
        1 => "warn",
        2..=255 => "error",
    };

    let levels = if let Ok(level) = std::env::var("LOG") {
        level
    } else {
        [
            format!("vector={}", level),
            format!("codec={}", level),
            format!("file_source={}", level),
            format!("tower_limit=trace"),
        ]
        .join(",")
        .to_string()
    };

    let color = match opts.color.clone().unwrap_or(Color::Auto) {
        #[cfg(unix)]
        Color::Auto => atty::is(atty::Stream::Stdout),
        #[cfg(windows)]
        Color::Auto => false, // ANSI colors are not supported by cmd.exe
        Color::Always => true,
        Color::Never => false,
    };

    let (metrics_controller, metrics_sink) = metrics::build();

    trace::init(
        color,
        levels.as_str(),
        opts.metrics_addr.map(|_| metrics_sink),
    );

    sub_command.map(|s| {
        std::process::exit(match s {
            SubCommand::Validate(v) => validate(&v),
            SubCommand::List(l) => list::cmd(&l),
            SubCommand::Test(t) => unit_test::cmd(&t),
            SubCommand::Generate(g) => generate::cmd(&g),
        })
    });

    info!("Log level {:?} is enabled.", level);

    if let Some(threads) = opts.threads {
        if threads < 1 || threads > 4 {
            error!("The `threads` argument must be between 1 and 4 (inclusive).");
            std::process::exit(exitcode::CONFIG);
        }
    }

    let mut config_paths = config_paths::expand(opts.config_paths.clone()).unwrap_or_else(|| {
        std::process::exit(exitcode::CONFIG);
    });
    config_paths.sort();
    config_paths.dedup();

    if opts.watch_config {
        // Start listening for config changes immediately.
        vector::topology::config::watcher::config_watcher(
            config_paths.clone(),
            vector::topology::config::watcher::CONFIG_WATCH_DELAY,
        )
        .unwrap_or_else(|error| {
            error!(message = "Unable to start config watcher.", %error);
            std::process::exit(exitcode::CONFIG);
        });
    }

    info!(
        message = "Loading configs.",
        path = ?config_paths
    );

    let config = read_configs(&config_paths);
    let config = handle_config_errors(config);
    let config = config.unwrap_or_else(|| {
        std::process::exit(exitcode::CONFIG);
    });
    event::LOG_SCHEMA
        .set(config.global.log_schema.clone())
        .expect("Couldn't set schema");

    let mut rt = {
        let threads = opts.threads.unwrap_or(max(1, num_cpus::get()));
        let num_threads = min(4, threads);
        runtime::Runtime::with_thread_count(num_threads).expect("Unable to create async runtime")
    };

    let (metrics_trigger, metrics_tripwire) = stream_cancel::Tripwire::new();

    if let Some(metrics_addr) = opts.metrics_addr {
        debug!("Starting metrics server");

        rt.spawn(
            metrics::serve(&metrics_addr, metrics_controller)
                .instrument(info_span!("metrics", addr = ?metrics_addr))
                .select(metrics_tripwire)
                .map(|_| ())
                .map_err(|_| ()),
        );
    }

    info!(
        message = "Vector is starting.",
        version = built_info::PKG_VERSION,
        git_version = built_info::GIT_VERSION.unwrap_or(""),
        released = built_info::BUILT_TIME_UTC,
        arch = built_info::CFG_TARGET_ARCH
    );

    if opts.dry_run {
        info!("Dry run enabled, exiting after config validation.");
    }

    let pieces = topology::validate(&config, rt.executor()).unwrap_or_else(|| {
        std::process::exit(exitcode::CONFIG);
    });

    if opts.dry_run && !opts.require_healthy {
        info!("Config validated, exiting.");
        std::process::exit(exitcode::OK);
    }

    let result = topology::start_validated(config, pieces, &mut rt, opts.require_healthy);
    let (topology, mut graceful_crash) = result.unwrap_or_else(|| {
        std::process::exit(exitcode::CONFIG);
    });

    if opts.dry_run {
        info!("Healthchecks passed, exiting.");
        std::process::exit(exitcode::OK);
    }

    #[cfg(unix)]
    {
        let mut topology = topology;
        let sigint = Signal::new(SIGINT).flatten_stream();
        let sigterm = Signal::new(SIGTERM).flatten_stream();
        let sigquit = Signal::new(SIGQUIT).flatten_stream();
        let sighup = Signal::new(SIGHUP).flatten_stream();

        let mut signals = sigint.select(sigterm.select(sigquit.select(sighup)));

        let signal = loop {
            let signal = future::poll_fn(|| signals.poll());
            let crash = future::poll_fn(|| graceful_crash.poll());

            let next = signal
                .select2(crash)
                .wait()
                .map_err(|_| ())
                .expect("Neither stream errors");

            let signal = match next {
                future::Either::A((signal, _)) => signal.expect("Signal streams never end"),
                future::Either::B((_crash, _)) => SIGINT, // Trigger graceful shutdown if a component crashed
            };

            if signal != SIGHUP {
                break signal;
            }

            // Reload config
            info!(
                message = "Reloading configs.",
                path = ?config_paths
            );
            let config = read_configs(&config_paths);

            trace!("Parsing config");
            let config = handle_config_errors(config);
            if let Some(config) = config {
                let success =
                    topology.reload_config_and_respawn(config, &mut rt, opts.require_healthy);
                if !success {
                    error!("Reload was not successful.");
                }
            } else {
                error!("Reload aborted.");
            }
        };

        if signal == SIGINT || signal == SIGTERM {
            use futures::future::Either;

            info!("Shutting down.");
            let shutdown = topology.stop();
            metrics_trigger.cancel();

            match rt.block_on(shutdown.select2(signals.into_future())) {
                Ok(Either::A(_)) => { /* Graceful shutdown finished */ }
                Ok(Either::B(_)) => {
                    info!("Shutting down immediately.");
                    // Dropping the shutdown future will immediately shut the server down
                }
                Err(_) => unreachable!(),
            }
        } else if signal == SIGQUIT {
            info!("Shutting down immediately");
            drop(topology);
        } else {
            unreachable!();
        }
    }
    #[cfg(windows)]
    {
        let ctrl_c = tokio_signal::ctrl_c().flatten_stream().into_future();
        let crash = future::poll_fn(move || graceful_crash.poll());

        let interruption = rt
            .block_on(ctrl_c.select2(crash))
            .map_err(|_| ())
            .expect("Neither stream errors");

        use futures::future::Either;

        let ctrl_c = match interruption {
            Either::A(((_, ctrl_c_stream), _)) => ctrl_c_stream.into_future(),
            Either::B((_, ctrl_c)) => ctrl_c,
        };

        info!("Shutting down.");
        let shutdown = topology.stop();
        metrics_trigger.cancel();

        match rt.block_on(shutdown.select2(ctrl_c)) {
            Ok(Either::A(_)) => { /* Graceful shutdown finished */ }
            Ok(Either::B(_)) => {
                info!("Shutting down immediately.");
                // Dropping the shutdown future will immediately shut the server down
            }
            Err(_) => unreachable!(),
        }
    }

    rt.shutdown_now().wait().unwrap();
}

fn handle_config_errors(config: Result<Config, Vec<String>>) -> Option<Config> {
    match config {
        Err(errors) => {
            for error in errors {
                error!("Configuration error: {}", error);
            }
            None
        }
        Ok(config) => Some(config),
    }
}

fn read_configs(config_paths: &Vec<PathBuf>) -> Result<Config, Vec<String>> {
    let mut config = vector::topology::Config::empty();
    let mut errors = Vec::new();

    config_paths.iter().for_each(|p| {
        let file = if let Some(file) = open_config(&p) {
            file
        } else {
            errors.push(format!("Config file not found in path: {:?}.", p));
            return;
        };

        trace!(
            message = "Parsing config.",
            path = ?p
        );

        match Config::load(file).and_then(|n| config.append(n)) {
            Err(errs) => errors.extend(errs.iter().map(|e| format!("{:?}: {}", p, e))),
            _ => (),
        };
    });

    if let Err(mut errs) = config.expand_macros() {
        errors.append(&mut errs);
    }

    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(config)
    }
}

fn open_config(path: &Path) -> Option<File> {
    match File::open(path) {
        Ok(f) => Some(f),
        Err(error) => {
            if let std::io::ErrorKind::NotFound = error.kind() {
                error!(message = "Config file not found in path.", ?path);
                None
            } else {
                error!(message = "Error opening config file.", %error);
                None
            }
        }
    }
}

fn validate(opts: &Validate) -> exitcode::ExitCode {
    let paths = config_paths::expand(opts.paths.clone()).unwrap_or_else(|| {
        std::process::exit(exitcode::CONFIG);
    });

    for config_path in paths {
        let file = if let Some(file) = open_config(&config_path) {
            file
        } else {
            error!(
                message = "Failed to open config file.",
                path = ?config_path
            );
            return exitcode::CONFIG;
        };

        trace!(
            message = "Parsing config.",
            path = ?config_path
        );

        let config = vector::topology::Config::load(file);
        let config = handle_config_errors(config);
        let mut config = config.unwrap_or_else(|| {
            error!(
                message = "Failed to parse config file.",
                path = ?config_path
            );
            std::process::exit(exitcode::CONFIG);
        });
        if let Err(errs) = config.expand_macros() {
            for error in errs {
                error!("Parse error: {}", error);
            }
        }

        if opts.topology {
            let exit = match topology::builder::check(&config) {
                Err(errors) => {
                    for error in errors {
                        error!("Topology error: {}", error);
                    }
                    Some(exitcode::CONFIG)
                }
                Ok(warnings) => {
                    for warning in &warnings {
                        warn!("Topology warning: {}", warning);
                    }
                    if opts.deny_warnings && !warnings.is_empty() {
                        Some(exitcode::CONFIG)
                    } else {
                        None
                    }
                }
            };
            if exit.is_some() {
                error!(
                    message = "Failed to verify config file topology.",
                    path = ?config_path
                );
                return exit.unwrap();
            }
        }

        debug!(
            message = "Validation successful.",
            path = ?config_path
        );
    }

    exitcode::OK
}

#[allow(unused)]
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
