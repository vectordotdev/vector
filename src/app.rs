use crate::cli::{handle_config_errors, Color, LogFormat, Opts, RootOpts, SubCommand};
use crate::signal::SignalTo;
use crate::topology::RunningTopology;
use crate::{
    config, generate, heartbeat, list, metrics, signal, topology, trace, unit_test, validate,
};
use std::cmp::max;
use std::collections::HashMap;
use std::path::PathBuf;

use futures::StreamExt;
use tokio::sync::mpsc;

#[cfg(feature = "sources-host_metrics")]
use crate::sources::host_metrics;
#[cfg(feature = "api-client")]
use crate::top;
#[cfg(feature = "api")]
use crate::{api, internal_events::ApiStarted};

#[cfg(windows)]
use crate::service;

use crate::internal_events::{
    VectorConfigLoadFailed, VectorQuit, VectorRecoveryFailed, VectorReloadFailed, VectorReloaded,
    VectorStarted, VectorStopped,
};
use tokio::runtime;
use tokio::runtime::Runtime;

pub struct ApplicationConfig {
    pub config_paths: Vec<(PathBuf, config::FormatHint)>,
    pub topology: RunningTopology,
    pub graceful_crash: mpsc::UnboundedReceiver<()>,
    #[cfg(feature = "api")]
    pub api: config::api::Options,
}

pub struct Application {
    opts: RootOpts,
    pub config: ApplicationConfig,
    pub runtime: Runtime,
}

impl Application {
    pub fn prepare() -> Result<Self, exitcode::ExitCode> {
        let opts = Opts::get_matches();
        Self::prepare_from_opts(opts)
    }

    pub fn prepare_from_opts(opts: Opts) -> Result<Self, exitcode::ExitCode> {
        openssl_probe::init_ssl_cert_env_vars();

        let level = std::env::var("LOG").unwrap_or_else(|_| match opts.log_level() {
            "off" => "off".to_owned(),
            level => [
                format!("vector={}", level),
                format!("codec={}", level),
                format!("vrl={}", level),
                format!("file_source={}", level),
                "tower_limit=trace".to_owned(),
                format!("rdkafka={}", level),
            ]
            .join(","),
        });

        let root_opts = opts.root;

        let sub_command = opts.sub_command;

        let color = match root_opts.color {
            #[cfg(unix)]
            Color::Auto => atty::is(atty::Stream::Stdout),
            #[cfg(windows)]
            Color::Auto => false, // ANSI colors are not supported by cmd.exe
            Color::Always => true,
            Color::Never => false,
        };

        let json = match &root_opts.log_format {
            LogFormat::Text => false,
            LogFormat::Json => true,
        };

        trace::init(color, json, &level);

        metrics::init().expect("metrics initialization failed");

        if let Some(threads) = root_opts.threads {
            if threads < 1 {
                error!("The `threads` argument must be greater or equal to 1.");
                return Err(exitcode::CONFIG);
            }
        }

        let mut rt = {
            let threads = root_opts.threads.unwrap_or_else(|| max(1, num_cpus::get()));
            runtime::Builder::new()
                .threaded_scheduler()
                .enable_all()
                .core_threads(threads)
                .build()
                .expect("Unable to create async runtime")
        };

        let config = {
            let config_paths = root_opts.config_paths_with_formats();
            let watch_config = root_opts.watch_config;
            let require_healthy = root_opts.require_healthy;

            rt.block_on(async move {
                if let Some(s) = sub_command {
                    let code = match s {
                        SubCommand::Validate(v) => validate::validate(&v, color).await,
                        SubCommand::List(l) => list::cmd(&l),
                        SubCommand::Test(t) => unit_test::cmd(&t).await,
                        SubCommand::Generate(g) => generate::cmd(&g),
                        #[cfg(feature = "api-client")]
                        SubCommand::Top(t) => top::cmd(&t).await,
                        #[cfg(windows)]
                        SubCommand::Service(s) => service::cmd(&s),
                        #[cfg(feature = "vrl-cli")]
                        SubCommand::VRL(s) => vrl_cli::cmd::cmd(&s),
                    };

                    return Err(code);
                };

                info!(message = "Log level is enabled.", level = ?level);

                #[cfg(feature = "sources-host_metrics")]
                host_metrics::init_roots();

                let config_paths = config::process_paths(&config_paths).ok_or(exitcode::CONFIG)?;

                if watch_config {
                    // Start listening for config changes immediately.
                    config::watcher::spawn_thread(config_paths.iter().map(|(path, _)| path), None)
                        .map_err(|error| {
                            error!(message = "Unable to start config watcher.", %error);
                            exitcode::CONFIG
                        })?;
                }

                info!(
                    message = "Loading configs.",
                    path = ?config_paths
                );

                let mut config =
                    config::load_from_paths(&config_paths, false).map_err(handle_config_errors)?;

                config::LOG_SCHEMA
                    .set(config.global.log_schema.clone())
                    .expect("Couldn't set schema");

                if !config.healthchecks.enabled {
                    info!("Health checks are disabled.");
                }
                config.healthchecks.set_require_healthy(require_healthy);

                let diff = config::ConfigDiff::initial(&config);
                let pieces = topology::build_or_log_errors(&config, &diff, HashMap::new())
                    .await
                    .ok_or(exitcode::CONFIG)?;

                #[cfg(feature = "api")]
                let api = config.api;

                let result = topology::start_validated(config, diff, pieces).await;
                let (topology, graceful_crash) = result.ok_or(exitcode::CONFIG)?;

                Ok(ApplicationConfig {
                    config_paths,
                    topology,
                    graceful_crash,
                    #[cfg(feature = "api")]
                    api,
                })
            })
        }?;

        Ok(Application {
            opts: root_opts,
            config,
            runtime: rt,
        })
    }

    pub fn run(self) {
        let mut rt = self.runtime;

        let mut graceful_crash = self.config.graceful_crash;
        let mut topology = self.config.topology;

        let mut config_paths = self.config.config_paths;

        let opts = self.opts;

        #[cfg(feature = "api")]
        let api_config = self.config.api;

        // Any internal_logs sources will have grabbed a copy of the
        // early buffer by this point and set up a subscriber.
        crate::trace::stop_buffering();

        rt.block_on(async move {
            emit!(VectorStarted);
            tokio::spawn(heartbeat::heartbeat());

            #[cfg(feature = "api")]
            // assigned to prevent the API terminating when falling out of scope
            let api_server = if api_config.enabled {
                emit!(ApiStarted {
                    addr: api_config.address.unwrap(),
                    playground: api_config.playground
                });

                Some(api::Server::start(topology.config()))
            } else {
                info!(message="API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.");
                None
            };

            let signals = signal::signals();
            tokio::pin!(signals);
            let mut sources_finished = topology.sources_finished();

            let signal = loop {
                tokio::select! {
                Some(signal) = signals.next() => {
                    if signal == SignalTo::Reload {
                        // Reload paths
                        config_paths = config::process_paths(&opts.config_paths_with_formats()).unwrap_or(config_paths);
                        // Reload config
                        let new_config = config::load_from_paths(&config_paths, false).map_err(handle_config_errors).ok();

                        if let Some(mut new_config) = new_config {
                            new_config.healthchecks.set_require_healthy(opts.require_healthy);
                            match topology
                                .reload_config_and_respawn(new_config)
                                .await
                            {
                                Ok(true) => {
                                    #[cfg(feature="api")]
                                    if let Some(ref api_server) = api_server {
                                        api_server.update_config(topology.config())
                                    }

                                    emit!(VectorReloaded { config_paths: &config_paths })
                                },
                                Ok(false) => emit!(VectorReloadFailed),
                                // Trigger graceful shutdown for what remains of the topology
                                Err(()) => {
                                    emit!(VectorReloadFailed);
                                    emit!(VectorRecoveryFailed);
                                    break SignalTo::Shutdown;
                                }
                            }
                            sources_finished = topology.sources_finished();
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
                    tokio::select! {
                    _ = topology.stop() => (), // Graceful shutdown finished
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
}
