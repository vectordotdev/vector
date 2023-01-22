use std::{collections::HashMap, num::NonZeroUsize, path::PathBuf, sync::Arc};

use futures::StreamExt;
#[cfg(feature = "enterprise")]
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use once_cell::race::OnceNonZeroUsize;
use tokio::{
    runtime::{self, Runtime},
    sync::{mpsc, Mutex},
};
use tokio_stream::wrappers::UnboundedReceiverStream;

#[cfg(feature = "enterprise")]
use crate::config::enterprise::{
    attach_enterprise_components, report_configuration, EnterpriseError, EnterpriseMetadata,
    EnterpriseReporter,
};
#[cfg(not(windows))]
use crate::control_server::ControlServer;
#[cfg(not(feature = "enterprise-tests"))]
use crate::metrics;
#[cfg(windows)]
use crate::service;
#[cfg(feature = "api")]
use crate::{api, internal_events::ApiStarted};
use crate::{
    cli::{handle_config_errors, Color, LogFormat, Opts, RootOpts, SubCommand},
    config, generate, generate_schema, graph, heartbeat, list,
    signal::{self, SignalTo},
    topology::{self, ReloadOutcome, RunningTopology, TopologyController},
    trace, unit_test, validate,
};
#[cfg(feature = "api-client")]
use crate::{tap, top};

pub static WORKER_THREADS: OnceNonZeroUsize = OnceNonZeroUsize::new();

use crate::internal_events::{VectorQuit, VectorStarted, VectorStopped};

use tokio::sync::broadcast::error::RecvError;

pub struct ApplicationConfig {
    pub config_paths: Vec<config::ConfigPath>,
    pub topology: RunningTopology,
    pub graceful_crash_sender: mpsc::UnboundedSender<()>,
    pub graceful_crash_receiver: mpsc::UnboundedReceiver<()>,
    #[cfg(feature = "api")]
    pub api: config::api::Options,
    #[cfg(feature = "enterprise")]
    pub enterprise: Option<EnterpriseReporter<BoxFuture<'static, ()>>>,
    pub signal_handler: signal::SignalHandler,
    pub signal_rx: signal::SignalRx,
}

pub struct Application {
    opts: RootOpts,
    pub config: ApplicationConfig,
    pub runtime: Runtime,
}

impl Application {
    pub fn prepare() -> Result<Self, exitcode::ExitCode> {
        let opts = Opts::get_matches().map_err(|error| {
            // Printing to stdout/err can itself fail; ignore it.
            let _ = error.print();
            exitcode::USAGE
        })?;

        Self::prepare_from_opts(opts)
    }

    pub fn prepare_from_opts(opts: Opts) -> Result<Self, exitcode::ExitCode> {
        openssl_probe::init_ssl_cert_env_vars();

        let level = std::env::var("VECTOR_LOG")
            .or_else(|_| {
                warn!(message = "Use of $LOG is deprecated. Please use $VECTOR_LOG instead.");
                std::env::var("LOG")
            })
            .unwrap_or_else(|_| match opts.log_level() {
                "off" => "off".to_owned(),
                level => [
                    format!("vector={}", level),
                    format!("codec={}", level),
                    format!("vrl={}", level),
                    format!("file_source={}", level),
                    "tower_limit=trace".to_owned(),
                    format!("rdkafka={}", level),
                    format!("buffers={}", level),
                    format!("lapin={}", level),
                    format!("kube={}", level),
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

        #[cfg(not(feature = "enterprise-tests"))]
        metrics::init_global().expect("metrics initialization failed");

        let mut rt_builder = runtime::Builder::new_multi_thread();
        rt_builder.enable_all().thread_name("vector-worker");

        if let Some(threads) = root_opts.threads {
            if threads < 1 {
                #[allow(clippy::print_stderr)]
                {
                    eprintln!("The `threads` argument must be greater or equal to 1.");
                }
                return Err(exitcode::CONFIG);
            } else {
                WORKER_THREADS
                    .set(NonZeroUsize::new(threads).expect("already checked"))
                    .expect("double thread initialization");
                rt_builder.worker_threads(threads);
            }
        }

        let rt = rt_builder.build().expect("Unable to create async runtime");

        let config = {
            let config_paths = root_opts.config_paths_with_formats();
            let watch_config = root_opts.watch_config;
            let require_healthy = root_opts.require_healthy;

            rt.block_on(async move {
                trace::init(color, json, &level, root_opts.internal_log_rate_limit);
                info!(
                    message = "Internal log rate limit configured.",
                    internal_log_rate_secs = root_opts.internal_log_rate_limit
                );
                // Signal handler for OS and provider messages.
                let (mut signal_handler, signal_rx) = signal::SignalHandler::new();
                signal_handler.forever(signal::os_signals());

                if let Some(s) = sub_command {
                    let code = match s {
                        SubCommand::Generate(g) => generate::cmd(&g),
                        SubCommand::GenerateSchema => generate_schema::cmd(),
                        SubCommand::Graph(g) => graph::cmd(&g),
                        SubCommand::Config(c) => config::cmd(&c),
                        SubCommand::List(l) => list::cmd(&l),
                        SubCommand::Test(t) => unit_test::cmd(&t, &mut signal_handler).await,
                        #[cfg(windows)]
                        SubCommand::Service(s) => service::cmd(&s),
                        #[cfg(feature = "api-client")]
                        SubCommand::Top(t) => top::cmd(&t).await,
                        #[cfg(feature = "api-client")]
                        SubCommand::Tap(t) => tap::cmd(&t, signal_rx).await,

                        SubCommand::Validate(v) => validate::validate(&v, color).await,
                        #[cfg(feature = "vrl-cli")]
                        SubCommand::Vrl(s) => vrl_cli::cmd::cmd(&s),
                    };

                    return Err(code);
                };

                info!(message = "Log level is enabled.", level = ?level);

                let config_paths = config::process_paths(&config_paths).ok_or(exitcode::CONFIG)?;

                if watch_config {
                    // Start listening for config changes immediately.
                    config::watcher::spawn_thread(config_paths.iter().map(Into::into), None)
                        .map_err(|error| {
                            error!(message = "Unable to start config watcher.", %error);
                            exitcode::CONFIG
                        })?;
                }

                info!(
                    message = "Loading configs.",
                    paths = ?config_paths.iter().map(<&PathBuf>::from).collect::<Vec<_>>()
                );

                #[cfg(not(feature = "enterprise-tests"))]
                config::init_log_schema(&config_paths, true).map_err(handle_config_errors)?;

                let mut config = config::load_from_paths_with_provider_and_secrets(
                    &config_paths,
                    &mut signal_handler,
                )
                .await
                .map_err(handle_config_errors)?;

                if !config.healthchecks.enabled {
                    info!("Health checks are disabled.");
                }
                config.healthchecks.set_require_healthy(require_healthy);

                #[cfg(feature = "enterprise")]
                // Enable enterprise features, if applicable.
                let enterprise = match EnterpriseMetadata::try_from(&config) {
                    Ok(metadata) => {
                        let enterprise = EnterpriseReporter::new();

                        attach_enterprise_components(&mut config, &metadata);
                        enterprise.send(report_configuration(config_paths.clone(), metadata));

                        Some(enterprise)
                    }
                    Err(EnterpriseError::MissingApiKey) => {
                        error!("Enterprise configuration incomplete: missing API key.");
                        return Err(exitcode::CONFIG);
                    }
                    Err(_) => None,
                };

                let diff = config::ConfigDiff::initial(&config);
                let pieces = topology::build_or_log_errors(&config, &diff, HashMap::new())
                    .await
                    .ok_or(exitcode::CONFIG)?;

                #[cfg(feature = "api")]
                let api = config.api;

                let result = topology::start_validated(config, diff, pieces).await;
                let (topology, (graceful_crash_sender, graceful_crash_receiver)) =
                    result.ok_or(exitcode::CONFIG)?;

                Ok(ApplicationConfig {
                    config_paths,
                    topology,
                    graceful_crash_sender,
                    graceful_crash_receiver,
                    #[cfg(feature = "api")]
                    api,
                    #[cfg(feature = "enterprise")]
                    enterprise,
                    signal_handler,
                    signal_rx,
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

        let mut graceful_crash = UnboundedReceiverStream::new(self.config.graceful_crash_receiver);
        let topology = self.config.topology;

        let config_paths = self.config.config_paths;

        let opts = self.opts;

        #[cfg(feature = "api")]
        let api_config = self.config.api;

        #[cfg(feature = "enterprise")]
        let enterprise_reporter = self.config.enterprise;

        let mut signal_handler = self.config.signal_handler;
        let mut signal_rx = self.config.signal_rx;

        // Any internal_logs sources will have grabbed a copy of the
        // early buffer by this point and set up a subscriber.
        crate::trace::stop_early_buffering();

        rt.block_on(async move {
            emit!(VectorStarted);
            tokio::spawn(heartbeat::heartbeat());

            // Configure the API server, if applicable.
            #[cfg(feature = "api")]
            // Assigned to prevent the API terminating when falling out of scope.
            let api_server = if api_config.enabled {
                use std::sync::atomic::AtomicBool;

                let api_server = api::Server::start(topology.config(), topology.watch(), Arc::<AtomicBool>::clone(&topology.running));

                match api_server {
                    Ok(api_server) => {
                        emit!(ApiStarted {
                            addr: api_config.address.unwrap(),
                            playground: api_config.playground
                        });

                        Some(api_server)
                    }
                    Err(e) => {
                        error!("An error occurred that Vector couldn't handle: {}.", e);
                        let _ = self.config.graceful_crash_sender.send(());
                        None
                    }
                }
            } else {
                info!(message="API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.");
                None
            };

            let topology_controller = TopologyController {
                topology,
                config_paths,
                require_healthy: opts.require_healthy,
                #[cfg(feature = "enterprise")]
                enterprise_reporter,
                #[cfg(feature = "api")]
                api_server,
            };
            let topology_controller = Arc::new(Mutex::new(topology_controller));

            // If the relevant ENV var is set, start up the control server
            #[cfg(not(windows))]
            let control_server_pieces = if let Ok(path) = std::env::var("VECTOR_CONTROL_SOCKET_PATH") {
                let (shutdown_trigger, tripwire) = stream_cancel::Tripwire::new();
                match ControlServer::bind(path, Arc::clone(&topology_controller), tripwire) {
                    Ok(control_server) => {
                        let server_handle = tokio::spawn(control_server.run());
                        Some((shutdown_trigger, server_handle))
                    }
                    Err(error) => {
                        error!(message = "Error binding control server.", %error);
                        // TODO: We should exit non-zero here, but `Application::run` isn't set up
                        // that way, and we'd need to push everything up to the API server start
                        // into `Application::prepare`.
                        return
                    }
                }
            } else {
                None
            };

            let signal = loop {
                tokio::select! {
                    signal = signal_rx.recv() => {
                        match signal {
                            Ok(SignalTo::ReloadFromConfigBuilder(config_builder)) => {
                                let mut topology_controller = topology_controller.lock().await;
                                let new_config = config_builder.build().map_err(handle_config_errors).ok();
                                if let ReloadOutcome::FatalError = topology_controller.reload(new_config).await {
                                    break SignalTo::Shutdown;
                                }
                            }
                            Ok(SignalTo::ReloadFromDisk) => {
                                let mut topology_controller = topology_controller.lock().await;

                                // Reload paths
                                if let Some(paths) = config::process_paths(&opts.config_paths_with_formats()) {
                                    topology_controller.config_paths = paths;
                                }

                                // Reload config
                                let new_config = config::load_from_paths_with_provider_and_secrets(&topology_controller.config_paths, &mut signal_handler)
                                    .await
                                    .map_err(handle_config_errors).ok();

                                if let ReloadOutcome::FatalError = topology_controller.reload(new_config).await {
                                    break SignalTo::Shutdown;
                                }
                            },
                            Err(RecvError::Lagged(amt)) => warn!("Overflow, dropped {} signals.", amt),
                            Err(RecvError::Closed) => break SignalTo::Shutdown,
                            Ok(signal) => break signal,
                        }
                    }
                    // Trigger graceful shutdown if a component crashed, or all sources have ended.
                    _ = graceful_crash.next() => break SignalTo::Shutdown,
                    _ = sources_finished(Arc::clone(&topology_controller)) => {
                        info!("All sources have finished.");
                        break SignalTo::Shutdown
                    } ,
                    else => unreachable!("Signal streams never end"),
                }
            };

            // Shut down the control server, if running
            #[cfg(not(windows))]
            if let Some((shutdown_trigger, server_handle)) = control_server_pieces {
                drop(shutdown_trigger);
                server_handle.await.expect("control server task panicked").expect("control server error");
            }

            // Once any control server has stopped, we'll have the only reference to the topology
            // controller and can safely remove it from the Arc/Mutex to shut down the topology.
            let topology_controller = Arc::try_unwrap(topology_controller).expect("fail to unwrap topology controller").into_inner();

            match signal {
                SignalTo::Shutdown => {
                    emit!(VectorStopped);
                    tokio::select! {
                        _ = topology_controller.stop() => (), // Graceful shutdown finished
                        _ = signal_rx.recv() => {
                            // It is highly unlikely that this event will exit from topology.
                            emit!(VectorQuit);
                            // Dropping the shutdown future will immediately shut the server down
                        }
                    }
                }
                SignalTo::Quit => {
                    // It is highly unlikely that this event will exit from topology.
                    emit!(VectorQuit);
                    drop(topology_controller);
                }
                _ => unreachable!(),
            }
        });
    }
}

// The `sources_finished` method on `RunningTopology` only considers sources that are currently
// running at the time the method is called. This presents a problem when the set of running
// sources can change while we are waiting on the resulting future to resolve.
//
// This function resolves that issue by waiting in two stages. The first is the usual asynchronous
// wait for the future to complete. When it does, we know that all of the sources that existed when
// the future was built have finished, but we don't know if that's because they were replaced as
// part of a reload (in which case we don't want to return yet). To differentiate, we acquire the
// lock on the topology, create a new future, and check whether it resolves immediately or not. If
// it does resolve, we know all sources are truly finished because we held the lock during the
// check, preventing anyone else from adding new sources. If it does not resolve, that indicates
// that new sources have been added since our original call and we should start the process over to
// continue waiting.
async fn sources_finished(mutex: Arc<Mutex<TopologyController>>) {
    loop {
        // Do an initial async wait while the topology is running, making sure not the hold the
        // mutex lock while we wait on sources to finish.
        let initial = {
            let tc = mutex.lock().await;
            tc.topology.sources_finished()
        };
        initial.await;

        // Once the initial signal is tripped, hold lock on the topology while checking again. This
        // ensures that no other task is adding new sources.
        let top = mutex.lock().await;
        if top.topology.sources_finished().now_or_never().is_some() {
            return;
        } else {
            continue;
        }
    }
}
