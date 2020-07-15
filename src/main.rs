#[macro_use]
extern crate tracing;

mod cli;

use cli::{Color, LogFormat, Opts, SubCommand};
use futures::compat::Future01CompatExt;
use futures01::{future, Future, Stream};
use std::{
    cmp::max,
    fs::File,
    path::{Path, PathBuf},
};
#[cfg(unix)]
use tokio_signal::unix::{Signal, SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use topology::Config;
use vector::{
    config_paths, event, generate, list, metrics, runtime, topology, trace, unit_test, validate,
};

fn main() {
    openssl_probe::init_ssl_cert_env_vars();

    let root_opts = Opts::get_matches();
    let level = root_opts.log_level();

    let opts = root_opts.root;
    let sub_command = root_opts.sub_command;

    let levels = match std::env::var("LOG").ok() {
        Some(level) => level,
        None => match level {
            "off" => "off".to_string(),
            _ => [
                format!("vector={}", level),
                format!("codec={}", level),
                format!("file_source={}", level),
                "tower_limit=trace".to_owned(),
                format!("rdkafka={}", level),
            ]
            .join(","),
        },
    };

    let color = match opts.color {
        #[cfg(unix)]
        Color::Auto => atty::is(atty::Stream::Stdout),
        #[cfg(windows)]
        Color::Auto => false, // ANSI colors are not supported by cmd.exe
        Color::Always => true,
        Color::Never => false,
    };

    let json = match &opts.log_format {
        LogFormat::Text => false,
        LogFormat::Json => true,
    };

    trace::init(color, json, levels.as_str());

    metrics::init().expect("metrics initialization failed");

    info!("Log level {:?} is enabled.", level);

    if let Some(threads) = opts.threads {
        if threads < 1 {
            error!("The `threads` argument must be greater or equal to 1.");
            std::process::exit(exitcode::CONFIG);
        }
    }

    let mut rt = {
        let threads = opts.threads.unwrap_or_else(|| max(1, num_cpus::get()));
        runtime::Runtime::with_thread_count(threads).expect("Unable to create async runtime")
    };

    rt.block_on_std(async move {
        if let Some(s) = sub_command {
            std::process::exit(match s {
                SubCommand::Validate(v) => validate::validate(&v, color).await,
                SubCommand::List(l) => list::cmd(&l),
                SubCommand::Test(t) => unit_test::cmd(&t),
                SubCommand::Generate(g) => generate::cmd(&g),
            })
        };

        let config_paths = config_paths::prepare(opts.config_paths.clone()).unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });

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

        let read_config = read_configs(&config_paths);
        let maybe_config = handle_config_errors(read_config);
        let config = maybe_config.unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });
        event::LOG_SCHEMA
            .set(config.global.log_schema.clone())
            .expect("Couldn't set schema");

        info!(
            message = "Vector is starting.",
            version = built_info::PKG_VERSION,
            git_version = built_info::GIT_VERSION.unwrap_or(""),
            released = built_info::BUILT_TIME_UTC,
            arch = built_info::CFG_TARGET_ARCH
        );

        let diff = topology::ConfigDiff::initial(&config);
        let pieces = topology::validate(&config, &diff).await.unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });

        let result = topology::start_validated(config, diff, pieces, opts.require_healthy).await;
        let (topology, mut graceful_crash) = result.unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });

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
                let to_shutdown = future::poll_fn(|| graceful_crash.poll())
                    .map(|_| ())
                    .select(topology.sources_finished());

                let next = signal
                    .select2(to_shutdown)
                    .compat()
                    .await
                    .map_err(|_| ())
                    .expect("Neither stream errors");

                let signal = match next {
                    future::Either::A((signal, _)) => signal.expect("Signal streams never end"),
                    // Trigger graceful shutdown if a component crashed, or all sources have ended.
                    future::Either::B((_to_shutdown, _)) => SIGINT,
                };

                if signal != SIGHUP {
                    break signal;
                }

                // Reload config
                info!(
                    message = "Reloading configs.",
                    path = ?config_paths
                );
                let new_config = read_configs(&config_paths);

                trace!("Parsing config");
                let new_config = handle_config_errors(new_config);
                if let Some(new_config) = new_config {
                    match topology
                        .reload_config_and_respawn(new_config, opts.require_healthy)
                        .await
                    {
                        Ok(true) => (),
                        Ok(false) => error!("Reload was not successful."),
                        // Trigger graceful shutdown for what remains of the topology
                        Err(()) => break SIGINT,
                    }
                } else {
                    error!("Reload aborted.");
                }
            };

            if signal == SIGINT || signal == SIGTERM {
                use futures01::future::Either;

                info!("Shutting down.");
                let shutdown = topology.stop();

                match shutdown.select2(signals.into_future()).compat().await {
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
            let to_shutdown = future::poll_fn(move || graceful_crash.poll())
                .map(|_| ())
                .select(topology.sources_finished());

            let interruption = ctrl_c
                .select2(to_shutdown)
                .compat()
                .await
                .map_err(|_| ())
                .expect("Neither stream errors");

            use futures01::future::Either;

            let ctrl_c = match interruption {
                Either::A(((_, ctrl_c_stream), _)) => ctrl_c_stream.into_future(),
                Either::B((_, ctrl_c)) => ctrl_c,
            };

            info!("Shutting down.");
            let shutdown = topology.stop();

            match shutdown.select2(ctrl_c).compat().await {
                Ok(Either::A(_)) => { /* Graceful shutdown finished */ }
                Ok(Either::B(_)) => {
                    info!("Shutting down immediately.");
                    // Dropping the shutdown future will immediately shut the server down
                }
                Err(_) => unreachable!(),
            }
        }
    });

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

fn read_configs(config_paths: &[PathBuf]) -> Result<Config, Vec<String>> {
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

        if let Err(errs) = Config::load(file).and_then(|n| config.append(n)) {
            errors.extend(errs.iter().map(|e| format!("{:?}: {}", p, e)));
        }
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

#[allow(unused)]
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
