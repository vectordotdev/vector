#![allow(missing_docs)]
use std::{collections::HashMap, num::NonZeroUsize, path::PathBuf};

use exitcode::ExitCode;
use futures::StreamExt;
#[cfg(feature = "enterprise")]
use futures_util::future::BoxFuture;
use once_cell::race::OnceNonZeroUsize;
use tokio::{
    runtime::{self, Runtime},
    sync::mpsc,
};
use tokio_stream::wrappers::UnboundedReceiverStream;

#[cfg(feature = "enterprise")]
use crate::config::enterprise::{
    attach_enterprise_components, report_configuration, EnterpriseError, EnterpriseMetadata,
    EnterpriseReporter,
};
#[cfg(not(feature = "enterprise-tests"))]
use crate::metrics;
#[cfg(feature = "api")]
use crate::{api, internal_events::ApiStarted};
use crate::{
    cli::{handle_config_errors, LogFormat, Opts, RootOpts},
    config::{self, Config, ConfigPath},
    heartbeat,
    signal::{SignalHandler, SignalPair, SignalRx, SignalTo},
    topology::{
        self, ReloadOutcome, RunningTopology, SharedTopologyController, TopologyController,
    },
    trace,
};

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
}

pub struct Application {
    pub require_healthy: Option<bool>,
    pub config: ApplicationConfig,
    pub signals: SignalPair,
}

impl ApplicationConfig {
    pub async fn from_opts(
        opts: &RootOpts,
        signal_handler: &mut SignalHandler,
    ) -> Result<Self, ExitCode> {
        let config_paths = opts.config_paths_with_formats();

        let config = load_configs(
            &config_paths,
            opts.watch_config,
            opts.require_healthy,
            signal_handler,
        )
        .await?;

        Self::from_config(config_paths, config).await
    }

    pub async fn from_config(
        config_paths: Vec<ConfigPath>,
        config: Config,
    ) -> Result<Self, ExitCode> {
        // This is ugly, but needed to allow `config` to be mutable for building the enterprise
        // features, but also avoid a "does not need to be mutable" warning when the enterprise
        // feature is not enabled.
        #[cfg(feature = "enterprise")]
        let mut config = config;
        #[cfg(feature = "enterprise")]
        let enterprise = build_enterprise(&mut config, config_paths.clone())?;

        let diff = config::ConfigDiff::initial(&config);
        let pieces = topology::build_or_log_errors(&config, &diff, HashMap::new())
            .await
            .ok_or(exitcode::CONFIG)?;

        #[cfg(feature = "api")]
        let api = config.api;

        let result = topology::start_validated(config, diff, pieces).await;
        let (topology, (graceful_crash_sender, graceful_crash_receiver)) =
            result.ok_or(exitcode::CONFIG)?;

        Ok(Self {
            config_paths,
            topology,
            graceful_crash_sender,
            graceful_crash_receiver,
            #[cfg(feature = "api")]
            api,
            #[cfg(feature = "enterprise")]
            enterprise,
        })
    }

    /// Configure the API server, if applicable
    #[cfg(feature = "api")]
    pub fn setup_api(&self, runtime: &Runtime) -> Option<api::Server> {
        if self.api.enabled {
            match api::Server::start(
                self.topology.config(),
                self.topology.watch(),
                std::sync::Arc::clone(&self.topology.running),
                runtime,
            ) {
                Ok(api_server) => {
                    emit!(ApiStarted {
                        addr: self.api.address.unwrap(),
                        playground: self.api.playground
                    });

                    Some(api_server)
                }
                Err(e) => {
                    error!("An error occurred that Vector couldn't handle: {}.", e);
                    _ = self.graceful_crash_sender.send(());
                    None
                }
            }
        } else {
            info!(message="API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.");
            None
        }
    }
}

impl Application {
    pub fn run() {
        let (runtime, app) = Self::prepare_start().unwrap_or_else(|code| std::process::exit(code));

        runtime.block_on(app.run());
    }

    pub fn prepare_start() -> Result<(Runtime, StartedApplication), ExitCode> {
        Self::prepare().and_then(|(runtime, app)| app.start(&runtime).map(|app| (runtime, app)))
    }

    pub fn prepare() -> Result<(Runtime, Self), ExitCode> {
        let opts = Opts::get_matches().map_err(|error| {
            // Printing to stdout/err can itself fail; ignore it.
            _ = error.print();
            exitcode::USAGE
        })?;

        Self::prepare_from_opts(opts)
    }

    pub fn prepare_from_opts(opts: Opts) -> Result<(Runtime, Self), ExitCode> {
        init_global();

        let color = opts.root.color.use_color();

        init_logging(
            color,
            opts.root.log_format,
            opts.log_level(),
            opts.root.internal_log_rate_limit,
        );

        let runtime = build_runtime(opts.root.threads, "vector-worker")?;

        // Signal handler for OS and provider messages.
        let mut signals = SignalPair::new(&runtime);

        if let Some(sub_command) = &opts.sub_command {
            return Err(runtime.block_on(sub_command.execute(signals, color)));
        }

        let config = runtime.block_on(ApplicationConfig::from_opts(
            &opts.root,
            &mut signals.handler,
        ))?;

        Ok((
            runtime,
            Self {
                require_healthy: opts.root.require_healthy,
                config,
                signals,
            },
        ))
    }

    pub fn start(self, runtime: &Runtime) -> Result<StartedApplication, ExitCode> {
        // Any internal_logs sources will have grabbed a copy of the
        // early buffer by this point and set up a subscriber.
        crate::trace::stop_early_buffering();

        emit!(VectorStarted);
        runtime.spawn(heartbeat::heartbeat());

        let Self {
            require_healthy,
            config,
            signals,
        } = self;

        let topology_controller = SharedTopologyController::new(TopologyController {
            #[cfg(feature = "api")]
            api_server: config.setup_api(runtime),
            topology: config.topology,
            config_paths: config.config_paths.clone(),
            require_healthy,
            #[cfg(feature = "enterprise")]
            enterprise_reporter: config.enterprise,
        });

        Ok(StartedApplication {
            config_paths: config.config_paths,
            graceful_crash_receiver: config.graceful_crash_receiver,
            signals,
            topology_controller,
        })
    }
}

pub struct StartedApplication {
    pub config_paths: Vec<ConfigPath>,
    pub graceful_crash_receiver: mpsc::UnboundedReceiver<()>,
    pub signals: SignalPair,
    pub topology_controller: SharedTopologyController,
}

impl StartedApplication {
    pub async fn run(self) {
        self.main().await.shutdown().await
    }

    pub async fn main(self) -> FinishedApplication {
        let Self {
            config_paths,
            graceful_crash_receiver,
            signals,
            topology_controller,
        } = self;

        let mut graceful_crash = UnboundedReceiverStream::new(graceful_crash_receiver);

        let mut signal_handler = signals.handler;
        let mut signal_rx = signals.receiver;

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
                            if let Some(paths) = config::process_paths(&config_paths) {
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
                _ = TopologyController::sources_finished(topology_controller.clone()) => {
                    info!("All sources have finished.");
                    break SignalTo::Shutdown
                } ,
                else => unreachable!("Signal streams never end"),
            }
        };

        FinishedApplication {
            signal,
            signal_rx,
            topology_controller,
        }
    }
}

pub struct FinishedApplication {
    pub signal: SignalTo,
    pub signal_rx: SignalRx,
    pub topology_controller: SharedTopologyController,
}

impl FinishedApplication {
    pub async fn shutdown(self) {
        let FinishedApplication {
            signal,
            mut signal_rx,
            topology_controller,
        } = self;

        // At this point, we'll have the only reference to the shared topology controller and can
        // safely remove it from the wrapper to shut down the topology.
        let topology_controller = topology_controller
            .try_into_inner()
            .expect("fail to unwrap topology controller")
            .into_inner();

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
    }
}

pub fn init_global() {
    openssl_probe::init_ssl_cert_env_vars();

    #[cfg(not(feature = "enterprise-tests"))]
    metrics::init_global().expect("metrics initialization failed");
}

fn get_log_levels(default: &str) -> String {
    std::env::var("VECTOR_LOG")
        .or_else(|_| {
            std::env::var("LOG").map(|log| {
                warn!(
                    message =
                        "DEPRECATED: Use of $LOG is deprecated. Please use $VECTOR_LOG instead."
                );
                log
            })
        })
        .unwrap_or_else(|_| match default {
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
        })
}

pub fn build_runtime(threads: Option<usize>, thread_name: &str) -> Result<Runtime, ExitCode> {
    let mut rt_builder = runtime::Builder::new_multi_thread();
    rt_builder.enable_all().thread_name(thread_name);

    if let Some(threads) = threads {
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

    Ok(rt_builder.build().expect("Unable to create async runtime"))
}

pub async fn load_configs(
    config_paths: &[ConfigPath],
    watch_config: bool,
    require_healthy: Option<bool>,
    signal_handler: &mut SignalHandler,
) -> Result<Config, ExitCode> {
    let config_paths = config::process_paths(config_paths).ok_or(exitcode::CONFIG)?;

    if watch_config {
        // Start listening for config changes immediately.
        config::watcher::spawn_thread(config_paths.iter().map(Into::into), None).map_err(
            |error| {
                error!(message = "Unable to start config watcher.", %error);
                exitcode::CONFIG
            },
        )?;
    }

    info!(
        message = "Loading configs.",
        paths = ?config_paths.iter().map(<&PathBuf>::from).collect::<Vec<_>>()
    );

    let mut config =
        config::load_from_paths_with_provider_and_secrets(&config_paths, signal_handler)
            .await
            .map_err(handle_config_errors)?;
    #[cfg(not(feature = "enterprise-tests"))]
    config::init_log_schema(config.global.log_schema.clone(), true);

    if !config.healthchecks.enabled {
        info!("Health checks are disabled.");
    }
    config.healthchecks.set_require_healthy(require_healthy);

    Ok(config)
}

#[cfg(feature = "enterprise")]
// Enable enterprise features, if applicable.
fn build_enterprise(
    config: &mut Config,
    config_paths: Vec<ConfigPath>,
) -> Result<Option<EnterpriseReporter<BoxFuture<'static, ()>>>, ExitCode> {
    match EnterpriseMetadata::try_from(&*config) {
        Ok(metadata) => {
            let enterprise = EnterpriseReporter::new();

            attach_enterprise_components(config, &metadata);
            enterprise.send(report_configuration(config_paths, metadata));

            Ok(Some(enterprise))
        }
        Err(EnterpriseError::MissingApiKey) => {
            error!("Enterprise configuration incomplete: missing API key.");
            Err(exitcode::CONFIG)
        }
        Err(_) => Ok(None),
    }
}

pub fn init_logging(color: bool, format: LogFormat, log_level: &str, rate: u64) {
    let level = get_log_levels(log_level);
    let json = match format {
        LogFormat::Text => false,
        LogFormat::Json => true,
    };

    trace::init(color, json, &level, rate);
    debug!(
        message = "Internal log rate limit configured.",
        internal_log_rate_secs = rate,
    );
    info!(message = "Log level is enabled.", level = ?level);
}
