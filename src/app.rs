use crate::{
    cli::{handle_config_errors, Color, LogFormat, Opts, RootOpts, SubCommand},
    config,
    control::{Control, Controller},
    generate, heartbeat, list, metrics, signal,
    topology::{self, RunningTopology},
    trace, unit_test, validate,
};
use cfg_if::cfg_if;
use std::{collections::HashMap, path::PathBuf};

use tokio::{
    runtime::{self, Runtime},
    sync::mpsc,
};
use tokio_stream::wrappers::ReceiverStream;

use futures::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[cfg(feature = "sources-host_metrics")]
use crate::sources::host_metrics;
#[cfg(feature = "api-client")]
use crate::tap;
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

pub struct ApplicationConfig {
    pub config_paths: Vec<(PathBuf, config::FormatHint)>,
    pub topology: RunningTopology,
    pub graceful_crash: mpsc::UnboundedReceiver<()>,
    #[cfg(feature = "api")]
    pub api: config::api::Options,
    #[cfg(feature = "providers")]
    pub provider: Option<Box<dyn config::provider::ProviderConfig>>,
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

        metrics::init().expect("metrics initialization failed");
        trace::init(color, json, &level);

        if let Some(threads) = root_opts.threads {
            if threads < 1 {
                error!("The `threads` argument must be greater or equal to 1.");
                return Err(exitcode::CONFIG);
            }
        }

        let rt = {
            runtime::Builder::new_multi_thread()
                .enable_all()
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
                        #[cfg(feature = "api-client")]
                        SubCommand::Tap(t) => tap::cmd(&t).await,
                        #[cfg(windows)]
                        SubCommand::Service(s) => service::cmd(&s),
                        #[cfg(feature = "vrl-cli")]
                        SubCommand::Vrl(s) => vrl_cli::cmd::cmd(&s),
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

                config::init_log_schema(&config_paths, true).map_err(handle_config_errors)?;

                let mut config =
                    config::load_from_paths(&config_paths).map_err(handle_config_errors)?;

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

                #[cfg(feature = "providers")]
                let provider = config.provider.take();

                let result = topology::start_validated(config, diff, pieces).await;
                let (topology, graceful_crash) = result.ok_or(exitcode::CONFIG)?;

                Ok(ApplicationConfig {
                    config_paths,
                    topology,
                    graceful_crash,
                    #[cfg(feature = "api")]
                    api,
                    #[cfg(feature = "providers")]
                    provider,
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
        let rt = self.runtime;

        let mut graceful_crash = UnboundedReceiverStream::new(self.config.graceful_crash);
        let mut topology = self.config.topology;

        let mut config_paths = self.config.config_paths;

        let opts = self.opts;

        #[cfg(feature = "api")]
        let api_config = self.config.api;

        #[cfg(feature = "providers")]
        let provider = self.config.provider;

        // Any internal_logs sources will have grabbed a copy of the
        // early buffer by this point and set up a subscriber.
        crate::trace::stop_buffering();

        rt.block_on(async move {
            emit!(VectorStarted);
            tokio::spawn(heartbeat::heartbeat());

            // Configure the API server, if applicable.
            cfg_if! (
                if #[cfg(feature = "api")] {
                    // Assigned to prevent the API terminating when falling out of scope.
                    let api_server = if api_config.enabled {
                        emit!(ApiStarted {
                            addr: api_config.address.unwrap(),
                            playground: api_config.playground
                        });

                        Some(api::Server::start(topology.config(), topology.watch()))
                    } else {
                        info!(message="API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.");
                        None
                    };
                }
            );

            // Controller for handling control messages.
            let mut controller = Controller::new();

            // Handle OS signals.
            let signals = signal::signals();
            controller.handler(signals);

            // Configure the provider, if applicable.
            #[cfg(feature = "providers")]
            let mut _provider = config::provider::init_provider(provider)
                .await
                .map(|provider| controller.with_shutdown(ReceiverStream::new(provider)));

            let mut control_rx = controller
                .take_rx()
                .expect("couldn't get controller receiver");

            let mut sources_finished = topology.sources_finished();

            let control = loop {
                tokio::select! {
                    Some(control) = control_rx.recv() => {
                        match control {
                            // Receive new configuration, and apply. This is typically sent from
                            // a provider. Only topology components are applied; API or other
                            // providers can only be applied from the 'root' filesystem config.
                            Control::Config(mut new_config) => {
                                new_config.healthchecks.set_require_healthy(opts.require_healthy);
                                    match topology
                                        .reload_config_and_respawn(new_config)
                                        .await
                                    {
                                        Ok(true) => {
                                            #[cfg(feature = "api")]
                                            // Pass the new config to the API server.
                                            if let Some(ref api_server) = api_server {
                                                api_server.update_config(topology.config());
                                            }

                                            emit!(VectorReloaded { config_paths: &config_paths })
                                        },
                                        Ok(false) => emit!(VectorReloadFailed),
                                        // Trigger graceful shutdown for what remains of the topology
                                        Err(()) => {
                                            emit!(VectorReloadFailed);
                                            emit!(VectorRecoveryFailed);
                                            break Control::Shutdown;
                                        }
                                    }
                                    sources_finished = topology.sources_finished();
                            }
                            // Reload a configuration from the filesystem. If a new provider is
                            // given, the previous drops out of scope and will be cleaned up.
                            Control::Reload => {
                                // Reload paths
                                config_paths = config::process_paths(&opts.config_paths_with_formats()).unwrap_or(config_paths);
                                // Reload config
                                let new_config = config::load_from_paths(&config_paths).map_err(handle_config_errors).ok();

                                if let Some(mut new_config) = new_config {
                                    #[cfg(feature = "providers")]
                                    // If there's a new provider, (re)instantiate it.
                                    if new_config.provider.is_some() {
                                        _provider = config::provider::init_provider(new_config.provider.take())
                                            .await
                                            .map(|provider| controller.with_shutdown(ReceiverStream::new(provider)));
                                    }

                                    new_config.healthchecks.set_require_healthy(opts.require_healthy);
                                    match topology
                                        .reload_config_and_respawn(new_config)
                                        .await
                                    {
                                        Ok(true) => {
                                            #[cfg(feature = "api")]
                                            // Pass the new config to the API server.
                                            if let Some(ref api_server) = api_server {
                                                api_server.update_config(topology.config());
                                            }

                                            emit!(VectorReloaded { config_paths: &config_paths })
                                        },
                                        Ok(false) => emit!(VectorReloadFailed),
                                        // Trigger graceful shutdown for what remains of the topology
                                        Err(()) => {
                                            emit!(VectorReloadFailed);
                                            emit!(VectorRecoveryFailed);
                                            break Control::Shutdown;
                                        }
                                    }
                                    sources_finished = topology.sources_finished();
                                } else {
                                    emit!(VectorConfigLoadFailed);
                                }
                            }
                            _ => break control,
                        }
                    }
                    // Trigger graceful shutdown if a component crashed, or all sources have ended.
                    _ = graceful_crash.next() => break Control::Shutdown,
                    _ = &mut sources_finished => break Control::Shutdown,
                    else => unreachable!("control signals never end"),
                }
            };

            match control {
                Control::Shutdown => {
                    emit!(VectorStopped);
                    tokio::select! {
                        _ = topology.stop() => (), // Graceful shutdown finished
                        _ = control_rx.recv() => {
                            // It is highly unlikely that this event will exit from topology.
                            emit!(VectorQuit);
                            // Dropping the shutdown future will immediately shut the server down
                        }
                    }
                }
                Control::Quit => {
                    // It is highly unlikely that this event will exit from topology.
                    emit!(VectorQuit);
                    drop(topology);
                }
                _ => unreachable!(),
            }
        });
    }
}
