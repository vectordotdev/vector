#[macro_use]
extern crate tracing;
#[macro_use]
extern crate vector;

mod cli;

use cli::{Color, LogFormat, Opts, SubCommand};
use futures::{
    compat::{Future01CompatExt, Stream01CompatExt},
    StreamExt,
};
use std::cmp::max;
use tokio::{runtime, select};
use vector::{
    config::{self, ConfigDiff},
    generate, heartbeat,
    internal_events::{
        VectorConfigLoadFailed, VectorQuit, VectorRecoveryFailed, VectorReloadFailed,
        VectorReloaded, VectorStarted, VectorStopped,
    },
    list, metrics,
    signal::{self, SignalTo},
    topology, trace, unit_test, validate,
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
        runtime::Builder::new()
            .threaded_scheduler()
            .enable_all()
            .core_threads(threads)
            .build()
            .expect("Unable to create async runtime")
    };

    rt.block_on(async move {
        if let Some(s) = sub_command {
            std::process::exit(match s {
                SubCommand::Validate(v) => validate::validate(&v, color).await,
                SubCommand::List(l) => list::cmd(&l),
                SubCommand::Test(t) => unit_test::cmd(&t),
                SubCommand::Generate(g) => generate::cmd(&g),
            })
        };

        let config_paths = config::process_paths(&opts.config_paths).unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });

        if opts.watch_config {
            // Start listening for config changes immediately.
            config::watcher::spawn_thread(&config_paths, None).unwrap_or_else(|error| {
                error!(message = "Unable to start config watcher.", %error);
                std::process::exit(exitcode::CONFIG);
            });
        }

        info!(
            message = "Loading configs.",
            path = ?config_paths
        );

        let config = config::load_from_paths(&config_paths)
            .map_err(handle_config_errors)
            .unwrap_or_else(|()| {
                std::process::exit(exitcode::CONFIG);
            });

        vector::event::LOG_SCHEMA
            .set(config.global.log_schema.clone())
            .expect("Couldn't set schema");

        let diff = ConfigDiff::initial(&config);
        let pieces = topology::validate(&config, &diff).await.unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });

        let result =
            topology::start_validated(config, diff, pieces, opts.require_healthy).await;
        let (mut topology, graceful_crash) = result.unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });

        emit!(VectorStarted);
        tokio::spawn(heartbeat::heartbeat());

        let mut signals = signal::signals();
        let mut sources_finished = topology.sources_finished().compat();
        let mut graceful_crash = graceful_crash.compat();

        let signal = loop {
            select! {
                Some(signal) = signals.next() => {
                    if signal == SignalTo::Reload {
                        // Reload config
                        let new_config = config::load_from_paths(&config_paths).map_err(handle_config_errors).ok();

                        if let Some(new_config) = new_config {
                            match topology
                                .reload_config_and_respawn(new_config, opts.require_healthy)
                                .await
                            {
                                Ok(true) =>  emit!(VectorReloaded { config_paths: &config_paths }),
                                Ok(false) => emit!(VectorReloadFailed),
                                // Trigger graceful shutdown for what remains of the topology
                                Err(()) => {
                                    emit!(VectorReloadFailed);
                                    emit!(VectorRecoveryFailed);
                                    break SignalTo::Shutdown;
                                }
                            }
                        } else {
                            emit!(VectorConfigLoadFailed);
                        }
                    } else {
                        break signal;
                    }
                }
                // Trigger graceful shutdown if a component crashed, or all sources have ended.
                _ = graceful_crash.next() => break SignalTo::Shutdown,
                _ = &mut sources_finished => break SignalTo::Shutdown,
                else => unreachable!("Signal streams never end"),
            }
        };

        match signal {
            SignalTo::Shutdown => {
                emit!(VectorStopped);
                select! {
                    _ = topology.stop().compat() => (), // Graceful shutdown finished
                    _ = signals.next() => {
                        // It is highly unlikely that this event will exit from topology.
                        emit!(VectorQuit);
                        // Dropping the shutdown future will immediately shut the server down
                    }
                }
            }
            SignalTo::Quit => {
                // It is highly unlikely that this event will exit from topology.
                emit!(VectorQuit);
                drop(topology);
            }
            SignalTo::Reload => unreachable!(),
        }
    });
}

fn handle_config_errors(errors: Vec<String>) {
    for error in errors {
        error!("Configuration error: {}", error);
    }
}
