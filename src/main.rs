#[macro_use]
extern crate tracing;

use futures::{future, Future, Stream};
use std::{
    cmp::{max, min},
    fs::File,
    net::SocketAddr,
    path::{Path, PathBuf},
};
use structopt::StructOpt;
use tokio_signal::unix::{Signal, SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use topology::Config;
use tracing::{field, Dispatch};
use tracing_futures::Instrument;
use tracing_metrics::MetricsSubscriber;
use vector::{metrics, topology};

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
struct Opts {
    /// Read configuration from the specified file
    #[structopt(name = "config", value_name = "FILE", short, long)]
    config_path: PathBuf,

    /// Exit on startup if any sinks fail healthchecks
    #[structopt(short, long)]
    require_healthy: bool,

    /// Exit on startup after config verification and optional healthchecks run
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

fn main() {
    let opts = Opts::from_args();

    let level = match opts.quiet {
        0 => match opts.verbose {
            0 => "info",
            1 => "debug",
            2..=255 => "trace",
        },
        1 => "warn",
        2..=255 => "error",
    };

    let mut levels = [
        format!("vector={}", level),
        format!("codec={}", level),
        format!("file_source={}", level),
    ]
    .join(",")
    .to_string();

    if let Ok(level) = std::env::var("LOG") {
        let additional_level = ",".to_owned() + level.as_str();
        levels.push_str(&additional_level);
    };

    let color = match opts.color.clone().unwrap_or(Color::Auto) {
        Color::Auto => atty::is(atty::Stream::Stdout),
        Color::Always => true,
        Color::Never => false,
    };

    let subscriber = tracing_fmt::FmtSubscriber::builder()
        .with_ansi(color)
        .with_filter(tracing_fmt::filter::EnvFilter::from(levels.as_str()))
        .finish();
    tracing_env_logger::try_init().expect("init log adapter");

    let (metrics_controller, metrics_sink) = metrics::build();
    let dispatch = if opts.metrics_addr.is_some() {
        Dispatch::new(MetricsSubscriber::new(subscriber, metrics_sink))
    } else {
        Dispatch::new(subscriber)
    };

    tracing::dispatcher::with_default(&dispatch, || {
        info!("Log level {:?} is enabled.", level);

        if let Some(threads) = opts.threads {
            if threads < 1 || threads > 4 {
                error!("The `threads` argument must be between 1 and 4 (inclusive).");
                std::process::exit(exitcode::CONFIG);
            }
        }

        info!(
            message = "Loading config.",
            path = field::debug(&opts.config_path)
        );

        let file = if let Some(file) = open_config(&opts.config_path) {
            file
        } else {
            std::process::exit(exitcode::CONFIG);
        };

        trace!(
            message = "Parsing config.",
            path = field::debug(&opts.config_path)
        );

        let config = vector::topology::Config::load(file);
        let config = handle_config_errors(config);
        let config = config.unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });

        let mut rt = {
            let mut builder = tokio::runtime::Builder::new();

            let threads = opts.threads.unwrap_or(max(1, num_cpus::get()));
            builder.core_threads(min(4, threads));

            builder.build().expect("Unable to create async runtime")
        };

        let (metrics_trigger, metrics_tripwire) = stream_cancel::Tripwire::new();

        if let Some(metrics_addr) = opts.metrics_addr {
            debug!("Starting metrics server");

            rt.spawn(
                metrics::serve(&metrics_addr, metrics_controller)
                    .instrument(info_span!("metrics", addr = field::display(&metrics_addr)))
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

        let pieces = topology::validate(&config).unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });

        if opts.dry_run && !opts.require_healthy {
            info!("Config validated, exiting.");
            std::process::exit(exitcode::OK);
        }

        let result = topology::start_validated(config, pieces, &mut rt, opts.require_healthy);
        let (mut topology, mut graceful_crash) = result.unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });

        if opts.dry_run {
            info!("Healthchecks passed, exiting.");
            std::process::exit(exitcode::OK);
        }

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
                message = "Reloading config.",
                path = field::debug(&opts.config_path)
            );

            let file = if let Some(file) = open_config(&opts.config_path) {
                file
            } else {
                continue;
            };

            trace!("Parsing config");
            let config = vector::topology::Config::load(file);
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

        rt.shutdown_now().wait().unwrap();
    });
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

fn open_config(path: &Path) -> Option<File> {
    match File::open(path) {
        Ok(f) => Some(f),
        Err(error) => {
            if let std::io::ErrorKind::NotFound = error.kind() {
                error!(
                    message = "Config file not found in path.",
                    path = field::display(path.display()),
                );
                None
            } else {
                error!(message = "Error opening config file.", %error);
                None
            }
        }
    }
}

#[allow(unused)]
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
