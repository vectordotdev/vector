#![allow(missing_docs)]
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;
use std::{
    num::{NonZeroU64, NonZeroUsize},
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use exitcode::ExitCode;
use futures::StreamExt;
use tokio::{
    runtime::{self, Handle, Runtime},
    sync::{MutexGuard, broadcast::error::RecvError},
};
use tokio_stream::wrappers::UnboundedReceiverStream;

#[cfg(feature = "api")]
use crate::api;
#[cfg(feature = "api")]
use crate::internal_events::ApiStarted;
use crate::{
    cli::{LogFormat, Opts, RootOpts, WatchConfigMethod, handle_config_errors},
    config::{self, Component, ComponentConfig, ComponentKind, Config, ConfigPath},
    extra_context::ExtraContext,
    heartbeat,
    internal_events::{
        VectorConfigLoadError, VectorQuit, VectorStarted, VectorStopped, VectorStopping,
    },
    signal::{SignalHandler, SignalPair, SignalRx, SignalTo},
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
    pub signals: SignalPair,
}

impl ApplicationConfig {
    pub async fn from_opts(
        opts: &RootOpts,
        signal_handler: &mut SignalHandler,
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

        let config = load_configs(
            &config_paths,
            watcher_conf,
            opts.require_healthy,
            opts.data_dir.clone(),
            opts.allow_empty_config,
            graceful_shutdown_duration,
            signal_handler,
        )
        .await?;

        Self::from_config(config_paths, config, extra_context).await
    }

    pub async fn from_config(
        config_paths: Vec<ConfigPath>,
        config: Config,
        extra_context: ExtraContext,
    ) -> Result<Self, ExitCode> {
        #[cfg(feature = "api")]
        let api = config.api;

        let (topology, graceful_crash_receiver) =
            RunningTopology::start_init_validated(config, extra_context.clone())
                .await
                .ok_or(exitcode::CONFIG)?;

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
        let mut signals = SignalPair::new(&runtime);

        if let Some(sub_command) = &opts.sub_command {
            // Combine root and subcommand flags before setting the global once.
            config::set_env_var_interpolation(
                opts.root.dangerously_allow_env_var_interpolation
                    || sub_command.dangerously_allow_env_var_interpolation(),
            );
            return Err(runtime.block_on(sub_command.execute(
                signals,
                color,
                opts.root.data_dir.as_deref(),
            )));
        }

        config::set_env_var_interpolation(opts.root.dangerously_allow_env_var_interpolation);

        let config = runtime.block_on(ApplicationConfig::from_opts(
            &opts.root,
            &mut signals.handler,
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
            data_dir: root_opts.data_dir,
        })
    }
}

pub struct StartedApplication {
    pub config_paths: Vec<ConfigPath>,
    pub internal_topologies: Vec<RunningTopology>,
    pub graceful_crash_receiver: ShutdownErrorReceiver,
    pub signals: SignalPair,
    pub topology_controller: SharedTopologyController,
    pub allow_empty_config: bool,
    pub data_dir: Option<PathBuf>,
}

impl StartedApplication {
    pub async fn run(self) -> ExitStatus {
        self.main().await.shutdown().await
    }

    pub async fn main(self) -> FinishedApplication {
        let Self {
            config_paths,
            graceful_crash_receiver,
            signals,
            topology_controller,
            internal_topologies,
            allow_empty_config,
            data_dir,
        } = self;

        let mut graceful_crash = UnboundedReceiverStream::new(graceful_crash_receiver);

        let mut signal_handler = signals.handler;
        let mut signal_rx = signals.receiver;

        let signal = loop {
            let has_sources = !topology_controller.lock().await.topology.config.is_empty();
            tokio::select! {
                signal = signal_rx.recv() => if let Some(signal) = handle_signal(
                    signal,
                    &topology_controller,
                    &config_paths,
                    &mut signal_handler,
                    allow_empty_config,
                    data_dir.as_deref(),
                ).await {
                    break signal;
                },
                // Trigger graceful shutdown if a component crashed, or all sources have ended.
                error = graceful_crash.next() => break SignalTo::Shutdown(error),
                _ = TopologyController::sources_finished(topology_controller.clone()), if has_sources => {
                    info!("All sources have finished.");
                    break SignalTo::Shutdown(None)
                } ,
                else => unreachable!("Signal streams never end"),
            }
        };

        FinishedApplication {
            signal,
            signal_rx,
            topology_controller,
            internal_topologies,
        }
    }
}

async fn handle_signal(
    signal: Result<SignalTo, RecvError>,
    topology_controller: &SharedTopologyController,
    config_paths: &[ConfigPath],
    signal_handler: &mut SignalHandler,
    allow_empty_config: bool,
    data_dir: Option<&Path>,
) -> Option<SignalTo> {
    match signal {
        Ok(SignalTo::ReloadComponents(components_to_reload)) => {
            let mut topology_controller = topology_controller.lock().await;
            topology_controller
                .topology
                .extend_reload_set(components_to_reload);

            // Reload paths
            if let Some(paths) = config::process_paths(config_paths) {
                topology_controller.config_paths = paths;
            }

            // Reload config
            let new_config = config::load_from_paths_with_provider_and_secrets(
                &topology_controller.config_paths,
                signal_handler,
                allow_empty_config,
                data_dir,
            )
            .await;

            reload_config_from_result(topology_controller, new_config).await
        }
        Ok(SignalTo::ReloadFromConfigBuilder(mut config_builder)) => {
            if let Some(data_dir) = data_dir {
                config_builder.set_data_dir(data_dir);
            }
            let topology_controller = topology_controller.lock().await;
            reload_config_from_result(topology_controller, config_builder.build()).await
        }
        Ok(SignalTo::ReloadFromDisk) => {
            let mut topology_controller = topology_controller.lock().await;

            // Reload paths
            if let Some(paths) = config::process_paths(config_paths) {
                topology_controller.config_paths = paths;
            }

            // Reload config
            let new_config = config::load_from_paths_with_provider_and_secrets(
                &topology_controller.config_paths,
                signal_handler,
                allow_empty_config,
                data_dir,
            )
            .await;

            if let Ok(ref config) = new_config {
                // Find all transforms that have external files to watch
                let transform_keys_to_reload = config.transform_keys_with_external_files();

                // Add these transforms to reload set
                if !transform_keys_to_reload.is_empty() {
                    info!(
                        message = "Reloading transforms with external files.",
                        count = transform_keys_to_reload.len()
                    );
                    topology_controller
                        .topology
                        .extend_reload_set(transform_keys_to_reload);
                }
            }

            reload_config_from_result(topology_controller, new_config).await
        }
        Ok(SignalTo::ReloadEnrichmentTables) => {
            let topology_controller = topology_controller.lock().await;

            topology_controller
                .topology
                .reload_enrichment_tables()
                .await;
            None
        }
        Err(RecvError::Lagged(amt)) => {
            warn!("Overflow, dropped {amt} signals.");
            None
        }
        Err(RecvError::Closed) => Some(SignalTo::Shutdown(None)),
        Ok(signal) => Some(signal),
    }
}

async fn reload_config_from_result(
    mut topology_controller: MutexGuard<'_, TopologyController>,
    config: Result<Config, Vec<String>>,
) -> Option<SignalTo> {
    match config {
        Ok(new_config) => match topology_controller.reload(new_config).await {
            ReloadOutcome::FatalError(error) => Some(SignalTo::Shutdown(Some(error))),
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
    pub signal: SignalTo,
    pub signal_rx: SignalRx,
    pub topology_controller: SharedTopologyController,
    pub internal_topologies: Vec<RunningTopology>,
}

impl FinishedApplication {
    pub async fn shutdown(self) -> ExitStatus {
        let FinishedApplication {
            signal,
            signal_rx,
            topology_controller,
            internal_topologies,
        } = self;

        // At this point, we'll have the only reference to the shared topology controller and can
        // safely remove it from the wrapper to shut down the topology.
        let topology_controller = topology_controller
            .try_into_inner()
            .expect("fail to unwrap topology controller")
            .into_inner();

        let status = match signal {
            SignalTo::Shutdown(triggering_error) => {
                Self::stop(topology_controller, signal_rx, triggering_error.is_none()).await
            }
            SignalTo::Quit => Self::quit(),
            _ => unreachable!(),
        };

        for topology in internal_topologies {
            topology.stop().await;
        }

        status
    }

    async fn stop(
        topology_controller: TopologyController,
        mut signal_rx: SignalRx,
        clean_shutdown: bool,
    ) -> ExitStatus {
        emit!(VectorStopping);
        tokio::select! {
            drained = topology_controller.stop() => {
                emit!(VectorStopped);
                if clean_shutdown && drained {
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
            _ = signal_rx.recv() => Self::quit(),
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

#[allow(clippy::too_many_arguments)]
pub async fn load_configs(
    config_paths: &[ConfigPath],
    watcher_conf: Option<config::watcher::WatcherConfig>,
    require_healthy: Option<bool>,
    data_dir: Option<PathBuf>,
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
        data_dir.as_deref(),
    )
    .await
    .map_err(handle_config_errors)?;

    let mut watched_component_paths = Vec::new();

    if let Some(watcher_conf) = watcher_conf {
        for (name, component) in config.components() {
            let files = match &component {
                Component::Source(_) => continue,
                Component::Transform(transform) => transform.inner.files_to_watch(),
                Component::Sink(sink) => sink.inner.files_to_watch(),
                Component::EnrichmentTable(table) => table.inner.files_to_watch(),
            };
            let component_config = ComponentConfig::new(
                files.iter().map(|path| (*path).clone()).collect(),
                name.clone(),
                component.kind(),
            );
            watched_component_paths.push(component_config);

            if let Component::EnrichmentTable(table) = &component
                && table.as_sink(name).is_some()
            {
                let sink_component_config = ComponentConfig::new(
                    files.into_iter().cloned().collect(),
                    name.clone(),
                    ComponentKind::Sink,
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
            signal_handler.clone_tx(),
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

#[cfg(all(test, feature = "sources-demo_logs", feature = "sinks-blackhole"))]
mod data_dir_tests {
    use clap::Parser;

    use super::*;

    fn config_text(data_dir: &Path, sink: &str) -> String {
        let data_dir = serde_json::to_string(data_dir).unwrap();
        format!(
            "data_dir: {data_dir}\nsources:\n  input:\n    type: demo_logs\n    format: shuffle\n    lines: [\"log\"]\nsinks:\n  {sink}:\n    type: blackhole\n    inputs: [input]\n"
        )
    }

    #[tokio::test]
    async fn cli_data_dir_survives_topology_reload_signals() {
        const CHILD: &str = "VECTOR_TEST_DATA_DIR_RELOAD_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .env(CHILD, "1")
                .env_remove("VECTOR_DATA_DIR")
                .args([
                    "--exact",
                    "app::data_dir_tests::cli_data_dir_survives_topology_reload_signals",
                    "--nocapture",
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            assert!(
                String::from_utf8_lossy(&output.stdout)
                    .contains("topology reload assertions completed")
            );
            return;
        }

        crate::test_util::trace_init();
        let directory = tempfile::tempdir().unwrap();
        let config_file = directory.path().join("vector.yaml");
        let override_dir = directory.path().join("override-state");
        std::fs::create_dir(&override_dir).unwrap();
        std::fs::write(
            &config_file,
            config_text(&directory.path().join("configured-state"), "initial"),
        )
        .unwrap();
        let opts = RootOpts::try_parse_from([
            "vector",
            "--config",
            config_file.to_str().unwrap(),
            "--data-dir",
            override_dir.to_str().unwrap(),
        ])
        .unwrap();
        let (mut signals, _receiver) = SignalHandler::new();
        let application =
            ApplicationConfig::from_opts(&opts, &mut signals, ExtraContext::default())
                .await
                .unwrap();
        let paths = application.config_paths.clone();
        let controller = SharedTopologyController::new(TopologyController {
            topology: application.topology,
            config_paths: paths.clone(),
            require_healthy: opts.require_healthy,
            #[cfg(feature = "api")]
            api_server: None,
            extra_context: ExtraContext::default(),
        });

        for replacement in ["disk", "provider", "components"] {
            let text = config_text(&directory.path().join(replacement), replacement);
            let signal = if replacement == "provider" {
                let builder = config::loading::ConfigBuilderLoader::default()
                    .load_from_input(text.as_bytes(), config::Format::Yaml)
                    .unwrap();
                SignalTo::ReloadFromConfigBuilder(builder)
            } else {
                std::fs::write(&config_file, text).unwrap();
                if replacement == "disk" {
                    SignalTo::ReloadFromDisk
                } else {
                    SignalTo::ReloadComponents(std::collections::HashSet::new())
                }
            };
            let outcome = tokio::time::timeout(
                Duration::from_secs(10),
                handle_signal(
                    Ok(signal),
                    &controller,
                    &paths,
                    &mut signals,
                    false,
                    opts.data_dir.as_deref(),
                ),
            )
            .await
            .expect("topology reload timed out");
            assert!(outcome.is_none());
            let current = controller.lock().await;
            // A retained data_dir alone is insufficient: the new topology must actually
            // have replaced the old one, rather than rolling back a rejected reload.
            assert!(
                current
                    .topology
                    .config
                    .sink(&config::ComponentKey::from(replacement))
                    .is_some()
            );
            assert_eq!(
                current.topology.config.global.data_dir.as_ref(),
                Some(&override_dir)
            );
        }
        let controller = controller
            .try_into_inner()
            .expect("no other controller owner")
            .into_inner();
        assert!(controller.stop().await);
        #[allow(
            clippy::print_stdout,
            reason = "The parent verifies the isolated reload assertions ran."
        )]
        {
            println!("topology reload assertions completed");
        }
    }
}
