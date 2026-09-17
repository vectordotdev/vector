#![allow(missing_docs)]
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;
use std::{
    num::{NonZeroU64, NonZeroUsize},
    path::PathBuf,
    process::ExitStatus,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use exitcode::ExitCode;
use futures::StreamExt;
use tokio::{
    runtime::{self, Handle, Runtime},
    sync::watch,
};
use tokio_stream::wrappers::UnboundedReceiverStream;

#[cfg(feature = "api")]
use crate::api;
#[cfg(feature = "api")]
use crate::internal_events::ApiStarted;
use crate::{
    bootstrap::{Bootstrap, Interrupted},
    cli::{LogFormat, Opts, RootOpts, WatchConfigMethod, handle_config_errors},
    config::{self, ComponentConfig, ComponentType, Config, ConfigPath},
    extra_context::ExtraContext,
    heartbeat,
    internal_events::{
        VectorConfigLoadError, VectorQuit, VectorStarted, VectorStopped, VectorStopping,
    },
    signal::{
        ReloadConfig, ReloadPlan, ShutdownSignal, ShutdownState, SignalHandler, Signals,
        wait_for_immediate, wait_for_shutdown,
    },
    topology::{
        ReloadOutcome, RunningTopology, SharedTopologyController, ShutdownErrorReceiver,
        TopologyController,
    },
    trace,
};

static WORKER_THREADS: AtomicUsize = AtomicUsize::new(0);

pub fn worker_threads() -> Option<NonZeroUsize> {
    NonZeroUsize::new(WORKER_THREADS.load(Ordering::Relaxed))
}

pub struct ApplicationConfig {
    pub config_paths: Vec<config::ConfigPath>,
    pub topology: RunningTopology,
    pub graceful_crash_receiver: ShutdownErrorReceiver,
    pub internal_topologies: Vec<RunningTopology>,
    #[cfg(feature = "api")]
    pub api: config::api::Options,
    pub extra_context: ExtraContext,
}

pub struct Application {
    pub root_opts: RootOpts,
    pub config: ApplicationConfig,
    pub signals: Signals,
}

impl ApplicationConfig {
    pub async fn from_opts(
        opts: &RootOpts,
        signals: &mut Signals,
        extra_context: ExtraContext,
    ) -> Result<Self, ExitCode> {
        let config_paths = opts.config_paths_with_formats();

        let graceful_shutdown_duration = (!opts.no_graceful_shutdown_limit)
            .then(|| Duration::from_secs(u64::from(opts.graceful_shutdown_limit_secs)));

        let watcher_conf = if opts.watch_config {
            Some(watcher_config(
                opts.watch_config_method,
                opts.watch_config_poll_interval_seconds,
            ))
        } else {
            None
        };

        // Config loading can block on network I/O (provider/secrets); race it against
        // shutdown so a signal aborts startup immediately.
        let config = {
            let mut bootstrap = Bootstrap::new(&mut signals.shutdown);
            match bootstrap
                .phase(load_configs(
                    &config_paths,
                    watcher_conf,
                    opts.require_healthy,
                    opts.allow_empty_config,
                    graceful_shutdown_duration,
                    &mut signals.handler,
                ))
                .await
            {
                // Shutdown (or closed channel) during loading: exit like a running Vector.
                Err(Interrupted) => return Err(exitcode::OK),
                Ok(config) => config?,
            }
        };

        Self::from_config(config_paths, config, extra_context, &mut signals.shutdown).await
    }

    pub async fn from_config(
        config_paths: Vec<ConfigPath>,
        config: Config,
        extra_context: ExtraContext,
        shutdown: &mut watch::Receiver<ShutdownState>,
    ) -> Result<Self, ExitCode> {
        #[cfg(feature = "api")]
        let api = config.api;

        let mut bootstrap = Bootstrap::new(shutdown);

        // Topology start can block on network I/O (healthchecks, API probes); race it
        // against shutdown so a signal aborts startup immediately.
        let (topology, graceful_crash_receiver) = match bootstrap
            .phase(RunningTopology::start_init_validated(
                config,
                extra_context.clone(),
            ))
            .await
        {
            // Shutdown (or closed channel) during startup: no topology to drain, exit
            // like a running Vector.
            Err(Interrupted) => return Err(exitcode::OK),
            Ok(Some(topology)) => topology,
            Ok(None) => return Err(exitcode::CONFIG),
        };

        Ok(Self {
            config_paths,
            topology,
            graceful_crash_receiver,
            internal_topologies: Vec::new(),
            #[cfg(feature = "api")]
            api,
            extra_context,
        })
    }

    pub async fn add_internal_config(
        &mut self,
        config: Config,
        extra_context: ExtraContext,
    ) -> Result<(), ExitCode> {
        let Some((topology, _)) =
            RunningTopology::start_init_validated(config, extra_context).await
        else {
            return Err(exitcode::CONFIG);
        };
        self.internal_topologies.push(topology);
        Ok(())
    }

    /// Configure the gRPC API server, if applicable
    #[cfg(feature = "api")]
    pub fn setup_api(&self, handle: &Handle) -> Option<api::GrpcServer> {
        if self.api.enabled {
            // Start gRPC server
            let api_server = handle.block_on(api::GrpcServer::start(
                self.topology.config(),
                self.topology.watch(),
            ));
            match api_server {
                Ok(server) => {
                    emit!(ApiStarted {
                        addr: server.addr()
                    });
                    Some(server)
                }
                Err(error) => {
                    let error = error.to_string();
                    error!(
                        message = "An error occurred that Vector couldn't handle.",
                        %error,
                        internal_log_rate_limit = false
                    );
                    // Trigger shutdown because the API was explicitly enabled but failed to start
                    // This ensures users don't run Vector thinking top/tap will work when they won't
                    _ = self
                        .topology
                        .abort_tx
                        .send(crate::signal::ShutdownError::ApiFailed { error });
                    None
                }
            }
        } else {
            info!(
                message = "API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`."
            );
            None
        }
    }
}

impl Application {
    pub fn run(extra_context: ExtraContext) -> ExitStatus {
        let (runtime, app) =
            Self::prepare_start(extra_context).unwrap_or_else(|code| std::process::exit(code));

        runtime.block_on(app.run())
    }

    pub fn prepare_start(
        extra_context: ExtraContext,
    ) -> Result<(Runtime, StartedApplication), ExitCode> {
        Self::prepare(extra_context)
            .and_then(|(runtime, app)| app.start(runtime.handle()).map(|app| (runtime, app)))
    }

    pub fn prepare(extra_context: ExtraContext) -> Result<(Runtime, Self), ExitCode> {
        let opts = Opts::get_matches().map_err(|error| {
            // Printing to stdout/err can itself fail; ignore it.
            _ = error.print();
            exitcode::USAGE
        })?;

        Self::prepare_from_opts(opts, extra_context)
    }

    pub fn prepare_from_opts(
        opts: Opts,
        extra_context: ExtraContext,
    ) -> Result<(Runtime, Self), ExitCode> {
        opts.root.init_global();

        crate::sources::util::set_max_decompressed_size_bytes(
            opts.root.max_decompressed_size_bytes,
        );

        let color = opts.root.color.use_color();

        init_logging(
            color,
            opts.root.log_format,
            opts.log_level(),
            opts.root.internal_log_rate_limit,
            opts.root.internal_logs_source_rate_limit,
        );

        #[cfg(unix)]
        if opts.root.raise_fd_limit {
            crate::cli::raise_file_descriptor_limit();
        }

        // Set global color preference for downstream modules
        crate::set_global_color(color);

        // Can only log this after initializing the logging subsystem
        if opts.root.openssl_no_probe {
            debug!(
                message = "Disabled probing and configuration of root certificate locations on the system for OpenSSL."
            );
        }

        let runtime = build_runtime(
            opts.root.threads,
            opts.root.chunk_size_events,
            "vector-worker",
        )?;

        // Signal handler for OS and provider messages.
        let mut signals = Signals::new(&runtime);

        if let Some(sub_command) = &opts.sub_command {
            // Combine root and subcommand flags before setting the global once.
            config::set_env_var_interpolation(
                opts.root.dangerously_allow_env_var_interpolation
                    || sub_command.dangerously_allow_env_var_interpolation(),
            );
            return Err(runtime.block_on(sub_command.execute(signals, color)));
        }

        config::set_env_var_interpolation(opts.root.dangerously_allow_env_var_interpolation);

        let config = runtime.block_on(ApplicationConfig::from_opts(
            &opts.root,
            &mut signals,
            extra_context,
        ))?;

        Ok((
            runtime,
            Self {
                root_opts: opts.root,
                config,
                signals,
            },
        ))
    }

    pub fn start(self, handle: &Handle) -> Result<StartedApplication, ExitCode> {
        // Any internal_logs sources will have grabbed a copy of the
        // early buffer by this point and set up a subscriber.
        crate::trace::stop_early_buffering();

        emit!(VectorStarted);
        handle.spawn(heartbeat::heartbeat());

        let Self {
            root_opts,
            config,
            signals,
        } = self;

        #[cfg(feature = "api")]
        let api_server = config.setup_api(handle);

        let topology_controller = SharedTopologyController::new(TopologyController {
            #[cfg(feature = "api")]
            api_server,
            topology: config.topology,
            config_paths: config.config_paths.clone(),
            require_healthy: root_opts.require_healthy,
            extra_context: config.extra_context,
        });

        Ok(StartedApplication {
            config_paths: config.config_paths,
            internal_topologies: config.internal_topologies,
            graceful_crash_receiver: config.graceful_crash_receiver,
            signals,
            topology_controller,
            allow_empty_config: root_opts.allow_empty_config,
        })
    }
}

pub struct StartedApplication {
    pub config_paths: Vec<ConfigPath>,
    pub internal_topologies: Vec<RunningTopology>,
    pub graceful_crash_receiver: ShutdownErrorReceiver,
    pub signals: Signals,
    pub topology_controller: SharedTopologyController,
    pub allow_empty_config: bool,
}

impl StartedApplication {
    pub async fn run(self) -> ExitStatus {
        self.main().await.shutdown().await
    }

    pub async fn main(self) -> FinishedApplication {
        let Self {
            config_paths,
            graceful_crash_receiver,
            mut signals,
            topology_controller,
            internal_topologies,
            allow_empty_config,
        } = self;

        let mut graceful_crash = UnboundedReceiverStream::new(graceful_crash_receiver);

        loop {
            let has_sources = !topology_controller.lock().await.topology.config.is_empty();
            tokio::select! {
                biased;
                // Handle shutdown before a queued reload that may block on config loading.
                _ = wait_for_shutdown(&mut signals.shutdown) => break,
                plan = signals.reloads.recv() => {
                    let mut reload_shutdown = signals.shutdown.clone();
                    tokio::select! {
                        biased;
                        _ = wait_for_immediate(&mut signals.shutdown) => break,
                        result = handle_reload(
                            plan,
                            &topology_controller,
                            &config_paths,
                            &mut signals.handler,
                            &mut reload_shutdown,
                            allow_empty_config,
                        ) => {
                            if let Some(signal) = result {
                                signals.handler.shutdown.send(signal);
                            }
                        }
                    }
                },
                // Trigger graceful shutdown if a component crashed, or all sources have ended.
                error = graceful_crash.next() => {
                    signals.handler.shutdown.send(error.map_or(
                        ShutdownSignal::Graceful,
                        ShutdownSignal::Failed,
                    ));
                },
                _ = TopologyController::sources_finished(topology_controller.clone()), if has_sources => {
                    info!("All sources have finished.");
                    signals.handler.shutdown.send(ShutdownSignal::Graceful);
                },
            }
        }

        FinishedApplication {
            signals,
            topology_controller,
            internal_topologies,
        }
    }
}

async fn handle_reload(
    plan: ReloadPlan,
    topology_controller: &SharedTopologyController,
    config_paths: &[ConfigPath],
    signal_handler: &mut SignalHandler,
    shutdown: &mut watch::Receiver<ShutdownState>,
    allow_empty_config: bool,
) -> Option<ShutdownSignal> {
    let mut topology_controller = topology_controller.lock().await;
    let needs_config = plan.config.is_some() || !plan.components.is_empty();
    if needs_config {
        if !plan.components.is_empty() {
            topology_controller
                .topology
                .extend_reload_set(plan.components);
        }

        let new_config = match plan.config {
            Some(ReloadConfig::Builder(builder)) => builder.build(),
            Some(ReloadConfig::Disk) | None => {
                if let Some(paths) = config::process_paths(config_paths) {
                    topology_controller.config_paths = paths;
                }

                // Loading is cancellation-safe; topology replacement below must finish or
                // roll back before graceful shutdown can drain it.
                tokio::select! {
                    biased;
                    _ = wait_for_shutdown(shutdown) => return None,
                    config = config::load_from_paths_with_provider_and_secrets(
                        &topology_controller.config_paths,
                        signal_handler,
                        allow_empty_config,
                    ) => config,
                }
            }
        };

        if plan.reload_external_files
            && let Ok(config) = &new_config
        {
            let transforms = config.transform_keys_with_external_files();
            if !transforms.is_empty() {
                info!(
                    message = "Reloading transforms with external files.",
                    count = transforms.len()
                );
                topology_controller.topology.extend_reload_set(transforms);
            }
        }

        if let Some(signal) = reload_config_from_result(&mut topology_controller, new_config).await
        {
            return Some(signal);
        }

    }

    if plan.enrichment_tables {
        topology_controller
            .topology
            .reload_enrichment_tables()
            .await;
    }
    None
}

async fn reload_config_from_result(
    topology_controller: &mut TopologyController,
    config: Result<Config, Vec<String>>,
) -> Option<ShutdownSignal> {
    match config {
        Ok(new_config) => match topology_controller.reload(new_config).await {
            ReloadOutcome::FatalError(error) => Some(ShutdownSignal::Failed(error)),
            _ => None,
        },
        Err(errors) => {
            handle_config_errors(errors);
            emit!(VectorConfigLoadError);
            None
        }
    }
}

pub struct FinishedApplication {
    pub signals: Signals,
    pub topology_controller: SharedTopologyController,
    pub internal_topologies: Vec<RunningTopology>,
}

impl FinishedApplication {
    pub async fn shutdown(self) -> ExitStatus {
        let FinishedApplication {
            mut signals,
            topology_controller,
            internal_topologies,
        } = self;

        // At this point, we'll have the only reference to the shared topology controller and can
        // safely remove it from the wrapper to shut down the topology.
        let topology_controller = topology_controller
            .try_into_inner()
            .expect("fail to unwrap topology controller")
            .into_inner();

        let state = wait_for_shutdown(&mut signals.shutdown).await;
        let status = if state.is_immediate() {
            Self::quit()
        } else {
            Self::stop(topology_controller, &mut signals.shutdown).await
        };

        for topology in internal_topologies {
            topology.stop().await;
        }

        status
    }

    async fn stop(
        topology_controller: TopologyController,
        shutdown: &mut watch::Receiver<ShutdownState>,
    ) -> ExitStatus {
        emit!(VectorStopping);
        tokio::select! {
            biased;
            // Repeated shutdown requests are durable even if both arrived before drain.
            _ = wait_for_immediate(shutdown) => Self::quit(),
            drained = topology_controller.stop() => {
                emit!(VectorStopped);
                if shutdown.borrow().error().is_none() && drained {
                    info!("All components shut down gracefully.");
                }
                ExitStatus::from_raw({
                    #[cfg(windows)]
                    {
                        exitcode::OK as u32
                    }
                    #[cfg(unix)]
                    exitcode::OK
                })
            }, // Graceful shutdown finished
        }
    }

    fn quit() -> ExitStatus {
        // It is highly unlikely that this event will exit from topology.
        emit!(VectorQuit);
        ExitStatus::from_raw({
            #[cfg(windows)]
            {
                exitcode::UNAVAILABLE as u32
            }
            #[cfg(unix)]
            exitcode::OK
        })
    }
}

fn get_log_levels(default: &str) -> String {
    std::env::var("VECTOR_LOG")
        .or_else(|_| {
            std::env::var("LOG").inspect(|_log| {
                warn!(
                    message =
                        "DEPRECATED: Use of $LOG is deprecated. Please use $VECTOR_LOG instead."
                );
            })
        })
        .unwrap_or_else(|_| default.into())
}

pub fn build_runtime(
    threads: Option<usize>,
    chunk_size_events: Option<NonZeroUsize>,
    thread_name: &str,
) -> Result<Runtime, ExitCode> {
    let mut rt_builder = runtime::Builder::new_multi_thread();
    rt_builder.max_blocking_threads(20_000);
    rt_builder.enable_all().thread_name(thread_name);

    let threads = threads.unwrap_or_else(crate::num_threads);
    if threads == 0 {
        error!("The `threads` argument must be greater or equal to 1.");
        return Err(exitcode::CONFIG);
    }
    WORKER_THREADS
        .compare_exchange(0, threads, Ordering::Acquire, Ordering::Relaxed)
        .unwrap_or_else(|_| panic!("double thread initialization"));
    rt_builder.worker_threads(threads);

    let chunk_size_events = chunk_size_events
        .map(NonZeroUsize::get)
        .unwrap_or(vector_lib::source_sender::DEFAULT_CHUNK_SIZE_EVENTS);

    let Some(source_sender_buffer_size) = threads.checked_mul(chunk_size_events) else {
        error!(
            "The `chunk_size_events` argument is too large for the configured number of threads."
        );
        return Err(exitcode::CONFIG);
    };
    let Some(ready_array_capacity) =
        chunk_size_events.checked_mul(crate::topology::builder::READY_ARRAY_CAPACITY_CHUNKS)
    else {
        error!("The `chunk_size_events` argument is too large.");
        return Err(exitcode::CONFIG);
    };

    vector_lib::source_sender::set_chunk_size_events(chunk_size_events);
    crate::topology::builder::set_source_sender_buffer_size(source_sender_buffer_size);
    crate::topology::builder::set_ready_array_capacity(ready_array_capacity);

    debug!(
        message = "Building runtime.",
        worker_threads = threads,
        chunk_size_events
    );
    Ok(rt_builder.build().expect("Unable to create async runtime"))
}

pub async fn load_configs(
    config_paths: &[ConfigPath],
    watcher_conf: Option<config::watcher::WatcherConfig>,
    require_healthy: Option<bool>,
    allow_empty_config: bool,
    graceful_shutdown_duration: Option<Duration>,
    signal_handler: &mut SignalHandler,
) -> Result<Config, ExitCode> {
    let config_paths = config::process_paths(config_paths).ok_or(exitcode::CONFIG)?;

    let watched_paths = config_paths
        .iter()
        .map(<&PathBuf>::from)
        .collect::<Vec<_>>();

    info!(
        message = "Loading configs.",
        paths = ?watched_paths
    );

    let mut config = config::load_from_paths_with_provider_and_secrets(
        &config_paths,
        signal_handler,
        allow_empty_config,
    )
    .await
    .map_err(handle_config_errors)?;

    let mut watched_component_paths = Vec::new();

    if let Some(watcher_conf) = watcher_conf {
        for (name, transform) in config.transforms() {
            let files = transform.inner.files_to_watch();
            let component_config = ComponentConfig::new(
                files.into_iter().cloned().collect(),
                name.clone(),
                ComponentType::Transform,
            );
            watched_component_paths.push(component_config);
        }

        for (name, sink) in config.sinks() {
            let files = sink.inner.files_to_watch();
            let component_config = ComponentConfig::new(
                files.into_iter().cloned().collect(),
                name.clone(),
                ComponentType::Sink,
            );
            watched_component_paths.push(component_config);
        }

        for (name, table) in config.enrichment_tables() {
            let files = table.inner.files_to_watch();
            let component_config = ComponentConfig::new(
                files.clone().into_iter().cloned().collect(),
                name.clone(),
                ComponentType::EnrichmentTable,
            );
            watched_component_paths.push(component_config);
            if table.as_sink(name).is_some() {
                let sink_component_config = ComponentConfig::new(
                    files.into_iter().cloned().collect(),
                    name.clone(),
                    ComponentType::Sink,
                );
                watched_component_paths.push(sink_component_config);
            }
        }

        info!(
            message = "Starting watcher.",
            paths = ?watched_paths
        );
        info!(
            message = "Components to watch.",
            paths = ?watched_component_paths
        );

        // Start listening for config changes.
        config::watcher::spawn_thread(
            watcher_conf,
            signal_handler.reloads.clone(),
            watched_paths,
            watched_component_paths,
            None,
        )
        .map_err(|error| {
            error!(message = "Unable to start config watcher.", %error);
            exitcode::CONFIG
        })?;
    }

    config::init_log_schema(config.global.log_schema.clone(), true);
    config::init_telemetry(config.global.telemetry.clone(), true);

    if !config.healthchecks.enabled {
        info!("Health checks are disabled.");
    }
    config.healthchecks.set_require_healthy(require_healthy);
    config.graceful_shutdown_duration = graceful_shutdown_duration;

    Ok(config)
}

pub fn init_logging(
    color: bool,
    format: LogFormat,
    log_level: &str,
    internal_log_rate_limit_secs: u64,
    internal_logs_source_rate_limit_secs: Option<NonZeroU64>,
) {
    let level = get_log_levels(log_level);
    let json = match format {
        LogFormat::Text => false,
        LogFormat::Json => true,
    };

    trace::init(
        color,
        json,
        &level,
        internal_log_rate_limit_secs,
        internal_logs_source_rate_limit_secs,
    );
    debug!(
        message = "Internal log rate limit configured.",
        internal_log_rate_limit_secs,
        internal_logs_source_rate_limit_secs =
            internal_logs_source_rate_limit_secs.map(NonZeroU64::get),
    );
    info!(message = "Log level is enabled.", ?level);
}

pub fn watcher_config(
    method: WatchConfigMethod,
    interval: NonZeroU64,
) -> config::watcher::WatcherConfig {
    match method {
        WatchConfigMethod::Recommended => config::watcher::WatcherConfig::RecommendedWatcher,
        WatchConfigMethod::Poll => config::watcher::WatcherConfig::PollWatcher(interval.into()),
    }
}
