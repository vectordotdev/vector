//! Topology contains all topology based types.
//!
//! Topology is broken up into two main sections. The first
//! section contains all the main topology types include `Topology`
//! and the ability to start, stop and reload a config. The second
//! part contains config related items including config traits for
//! each type of component.

pub mod builder;
pub mod config;
mod fanout;
mod task;
pub mod unit_test;

pub use self::config::Config;
pub use self::config::SinkContext;

use crate::topology::builder::Pieces;

use crate::buffers;
use crate::runtime;
use crate::shutdown::SourceShutdownCoordinator;
use futures::compat::Future01CompatExt;
use futures01::{
    future,
    sync::{mpsc, oneshot},
    Future, Stream,
};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::time::{Duration, Instant};
use tokio01::timer;
use tracing_futures::Instrument;

#[allow(dead_code)]
pub struct RunningTopology {
    inputs: HashMap<String, buffers::BufferInputCloner>,
    outputs: HashMap<String, fanout::ControlChannel>,
    source_tasks: HashMap<String, oneshot::SpawnHandle<(), ()>>,
    tasks: HashMap<String, oneshot::SpawnHandle<(), ()>>,
    shutdown_coordinator: SourceShutdownCoordinator,
    config: Config,
    abort_tx: mpsc::UnboundedSender<()>,
}

pub fn start(
    config: Config,
    rt: &mut runtime::Runtime,
    require_healthy: bool,
) -> Option<(RunningTopology, mpsc::UnboundedReceiver<()>)> {
    validate(&config, rt.executor())
        .and_then(|pieces| start_validated(config, pieces, rt, require_healthy))
}

pub fn start_validated(
    config: Config,
    mut pieces: Pieces,
    rt: &mut runtime::Runtime,
    require_healthy: bool,
) -> Option<(RunningTopology, mpsc::UnboundedReceiver<()>)> {
    let (abort_tx, abort_rx) = mpsc::unbounded();

    let mut running_topology = RunningTopology {
        inputs: HashMap::new(),
        outputs: HashMap::new(),
        config: Config::empty(),
        shutdown_coordinator: SourceShutdownCoordinator::new(),
        source_tasks: HashMap::new(),
        tasks: HashMap::new(),
        abort_tx,
    };

    if !running_topology.run_healthchecks(&config, &mut pieces, rt, require_healthy) {
        return None;
    }

    running_topology.spawn_all(config, pieces, rt);
    Some((running_topology, abort_rx))
}

pub fn validate(config: &Config, exec: runtime::TaskExecutor) -> Option<Pieces> {
    match builder::build_pieces(config, exec) {
        Err(errors) => {
            for error in errors {
                error!("Configuration error: {}", error);
            }
            None
        }
        Ok((new_pieces, warnings)) => {
            for warning in warnings {
                warn!("Configuration warning: {}", warning);
            }
            Some(new_pieces)
        }
    }
}

impl RunningTopology {
    /// Sends the shutdown signal to all sources and returns a future that resolves
    /// once all components (sources, transforms, and sinks) have finished shutting down.
    /// Transforms and sinks should shut down automatically once their input tasks finish.
    /// Note that this takes ownership of `self`, so once this function returns everything in the
    /// RunningTopology instance has been dropped except for the `tasks` map, which gets moved
    /// into the returned future and is used to poll for when the tasks have completed. One the
    /// returned future is dropped then everything from this RunningTopology instance is fully
    /// dropped.
    #[must_use]
    pub fn stop(self) -> impl Future<Item = (), Error = ()> {
        let mut running_tasks = self.tasks;

        let mut wait_handles = Vec::new();
        let mut check_handles = HashMap::new();

        for (name, task) in running_tasks.drain() {
            let task = task
                .or_else(|_| future::ok(())) // Consider an errored task to be shutdown
                .shared();

            wait_handles.push(task.clone());
            check_handles.insert(name, task);
        }
        let mut check_handles2 = check_handles.clone();

        let deadline = Instant::now() + Duration::from_secs(60);

        let timeout = timer::Delay::new(deadline)
            .map(move |_| {
                check_handles.retain(|_name, handle| {
                    handle.poll().map(|p| p.is_not_ready()).unwrap_or(false)
                });
                let remaining_components = check_handles.keys().cloned().collect::<Vec<_>>();

                error!(
                    "Failed to gracefully shut down in time. Killing: {}",
                    remaining_components.join(", ")
                );
            })
            .map_err(|err| panic!("Timer error: {:?}", err));

        let reporter = timer::Interval::new_interval(Duration::from_secs(5))
            .inspect(move |_| {
                check_handles2.retain(|_name, handle| {
                    handle.poll().map(|p| p.is_not_ready()).unwrap_or(false)
                });
                let remaining_components = check_handles2.keys().cloned().collect::<Vec<_>>();

                // TODO: replace with checked_duration_since once it's stable
                let time_remaining = if deadline > Instant::now() {
                    format!("{} seconds left", (deadline - Instant::now()).as_secs())
                } else {
                    "overdue".to_string()
                };

                info!(
                    "Shutting down... Waiting on: {}. {}",
                    remaining_components.join(", "),
                    time_remaining
                );
            })
            .filter(|_| false) // Run indefinitely without emitting items
            .into_future()
            .map(|_| ())
            .map_err(|(err, _)| panic!("Timer error: {:?}", err));

        let success = future::join_all(wait_handles)
            .map(|_| ())
            .map_err(|_: future::SharedError<()>| ());

        let shutdown_complete_future =
            future::select_all::<Vec<Box<dyn Future<Item = (), Error = ()> + Send>>>(vec![
                Box::new(timeout),
                Box::new(reporter),
                Box::new(success),
            ])
            .map(|_| ())
            .map_err(|_| ());

        // TODO: Once all Sources properly look for the ShutdownSignal, remove this in favor of
        // using 'deadline' instead.
        let source_shutdown_deadline = Instant::now() + Duration::from_secs(3);

        // Now kick off the shutdown process by shutting down the sources.
        let source_shutdown_complete = self
            .shutdown_coordinator
            .shutdown_all(source_shutdown_deadline);

        source_shutdown_complete
            .join(shutdown_complete_future)
            .map(|_| ())
    }

    pub fn reload_config_and_respawn(
        &mut self,
        new_config: Config,
        rt: &mut runtime::Runtime,
        require_healthy: bool,
    ) -> bool {
        if self.config.global.data_dir != new_config.global.data_dir {
            error!("data_dir cannot be changed while reloading config file; reload aborted. Current value: {:?}", self.config.global.data_dir);
            return false;
        }

        if self.config.global.dns_servers != new_config.global.dns_servers {
            error!("dns_servers cannot be changed while reloading config file; reload aborted. Current value: {:?}", self.config.global.dns_servers);
            return false;
        }

        match validate(&new_config, rt.executor()) {
            Some(mut new_pieces) => {
                if !self.run_healthchecks(&new_config, &mut new_pieces, rt, require_healthy) {
                    return false;
                }

                self.spawn_all(new_config, new_pieces, rt);
                true
            }

            None => false,
        }
    }

    fn run_healthchecks(
        &mut self,
        new_config: &Config,
        pieces: &mut Pieces,
        rt: &mut runtime::Runtime,
        require_healthy: bool,
    ) -> bool {
        let (_, sinks_to_change, sinks_to_add) =
            to_remove_change_add(&self.config.sinks, &new_config.sinks);

        let healthchecks = (&sinks_to_change | &sinks_to_add)
            .into_iter()
            .map(|name| pieces.healthchecks.remove(&name).unwrap())
            .collect::<Vec<_>>();
        let healthchecks = futures01::future::join_all(healthchecks).map(|_| ());

        info!("Running healthchecks.");
        if require_healthy {
            let jh = rt.spawn_handle(healthchecks.compat());
            let success = rt
                .block_on_std(jh)
                .expect("Task panicked or runtime shutdown unexpectedly");

            if success.is_ok() {
                info!("All healthchecks passed.");
                true
            } else {
                error!("Sinks unhealthy.");
                false
            }
        } else {
            rt.spawn(healthchecks);
            true
        }
    }

    fn spawn_all(&mut self, new_config: Config, mut new_pieces: Pieces, rt: &mut runtime::Runtime) {
        // Sources
        let (sources_to_remove, sources_to_change, sources_to_add) =
            to_remove_change_add(&self.config.sources, &new_config.sources);

        // First pass to tell the sources to shut down.
        let mut source_shutdown_complete_futures = Vec::new();
        // TODO: Once all Sources properly look for the ShutdownSignal, up this time limit to something
        // more like 30-60 seconds.
        info!("Waiting for up to 3 seconds for sources to finish shutting down");
        let deadline = Instant::now() + Duration::from_secs(3);
        for name in &sources_to_remove {
            info!("Removing source {:?}", name);

            self.tasks.remove(name).unwrap().forget();

            self.remove_outputs(name);
            source_shutdown_complete_futures
                .push(self.shutdown_coordinator.shutdown_source(name, deadline));
        }
        for name in &sources_to_change {
            info!("Rebuilding source {:?}", name);

            self.remove_outputs(name);
            source_shutdown_complete_futures
                .push(self.shutdown_coordinator.shutdown_source(name, deadline));
        }

        // Wait for the shutdowns to complete
        info!("Waiting for up to 3 seconds for sources to finish shutting down");
        rt.block_on(future::join_all(source_shutdown_complete_futures))
            .unwrap();

        // Second pass now that all sources have shut down for final cleanup and spawning
        // new versions of sources as needed.
        for name in &sources_to_remove {
            self.source_tasks.remove(name).wait().unwrap();
        }
        for name in &sources_to_change {
            self.source_tasks.remove(name).wait().unwrap();
            self.setup_outputs(name, &mut new_pieces);
            self.spawn_source(name, &mut new_pieces, rt);
        }

        for name in sources_to_add {
            info!("Starting source {:?}", name);

            self.setup_outputs(&name, &mut new_pieces);
            self.spawn_source(&name, &mut new_pieces, rt);
        }

        // Transforms
        let (transforms_to_remove, transforms_to_change, transforms_to_add) =
            to_remove_change_add(&self.config.transforms, &new_config.transforms);

        for name in transforms_to_remove {
            info!("Removing transform {:?}", name);

            self.tasks.remove(&name).unwrap().forget();

            self.remove_inputs(&name);
            self.remove_outputs(&name);
        }

        // Make sure all transform outputs are set up before another transform might try use
        // it as an input
        for name in &transforms_to_change {
            self.setup_outputs(&name, &mut new_pieces);
        }
        for name in &transforms_to_add {
            self.setup_outputs(&name, &mut new_pieces);
        }

        for name in transforms_to_change {
            info!("Rebuilding transform {:?}", name);

            self.replace_inputs(&name, &mut new_pieces);
            self.spawn_transform(&name, &mut new_pieces, rt);
        }

        for name in transforms_to_add {
            info!("Starting transform {:?}", name);

            self.setup_inputs(&name, &mut new_pieces);
            self.spawn_transform(&name, &mut new_pieces, rt);
        }

        // Sinks
        let (sinks_to_remove, sinks_to_change, sinks_to_add) =
            to_remove_change_add(&self.config.sinks, &new_config.sinks);

        for name in sinks_to_remove {
            info!("Removing sink {:?}", name);

            self.tasks.remove(&name).unwrap().forget();

            self.remove_inputs(&name);
        }

        for name in sinks_to_change {
            info!("Rebuilding sink {:?}", name);

            self.spawn_sink(&name, &mut new_pieces, rt);
            self.replace_inputs(&name, &mut new_pieces);
        }

        for name in sinks_to_add {
            info!("Starting sink {:?}", name);

            self.setup_inputs(&name, &mut new_pieces);
            self.spawn_sink(&name, &mut new_pieces, rt);
        }

        self.config = new_config;
    }

    fn spawn_sink(
        &mut self,
        name: &str,
        new_pieces: &mut builder::Pieces,
        rt: &mut runtime::Runtime,
    ) {
        let task = new_pieces.tasks.remove(name).unwrap();
        let span = info_span!("sink", name = %task.name(), r#type = %task.typetag());
        let task = handle_errors(task.instrument(span), self.abort_tx.clone());
        let spawned = oneshot::spawn(task, &rt.executor());
        if let Some(previous) = self.tasks.insert(name.to_string(), spawned) {
            previous.forget();
        }
    }

    fn spawn_transform(
        &mut self,
        name: &str,
        new_pieces: &mut builder::Pieces,
        rt: &mut runtime::Runtime,
    ) {
        let task = new_pieces.tasks.remove(name).unwrap();
        let span = info_span!("transform", name = %task.name(), r#type = %task.typetag());
        let task = handle_errors(task.instrument(span), self.abort_tx.clone());
        let spawned = oneshot::spawn(task, &rt.executor());
        if let Some(previous) = self.tasks.insert(name.to_string(), spawned) {
            previous.forget();
        }
    }

    fn spawn_source(
        &mut self,
        name: &str,
        new_pieces: &mut builder::Pieces,
        rt: &mut runtime::Runtime,
    ) {
        let task = new_pieces.tasks.remove(name).unwrap();
        let span = info_span!("source", name = %task.name(), r#type = %task.typetag());

        let task = handle_errors(task.instrument(span.clone()), self.abort_tx.clone());
        let spawned = oneshot::spawn(task, &rt.executor());
        if let Some(previous) = self.tasks.insert(name.to_string(), spawned) {
            previous.forget();
        }

        self.shutdown_coordinator
            .takeover_source(name, &mut new_pieces.shutdown_coordinator);

        let source_task = new_pieces.source_tasks.remove(name).unwrap();
        let source_task = handle_errors(source_task.instrument(span), self.abort_tx.clone());
        self.source_tasks.insert(
            name.to_string(),
            oneshot::spawn(source_task, &rt.executor()),
        );
    }

    fn remove_outputs(&mut self, name: &str) {
        self.outputs.remove(name);
    }

    fn remove_inputs(&mut self, name: &str) {
        self.inputs.remove(name);

        let sink_inputs = self.config.sinks.get(name).map(|s| &s.inputs);
        let trans_inputs = self.config.transforms.get(name).map(|t| &t.inputs);

        let inputs = sink_inputs.or(trans_inputs);

        if let Some(inputs) = inputs {
            for input in inputs {
                if let Some(output) = self.outputs.get(input) {
                    output
                        .unbounded_send(fanout::ControlMessage::Remove(name.to_string()))
                        .unwrap();
                    // std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    }

    fn setup_outputs(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let output = new_pieces.outputs.remove(name).unwrap();

        for (sink_name, sink) in &self.config.sinks {
            if sink.inputs.iter().any(|i| i == name) {
                output
                    .unbounded_send(fanout::ControlMessage::Add(
                        sink_name.clone(),
                        self.inputs[sink_name].get(),
                    ))
                    .unwrap();
            }
        }
        for (transform_name, transform) in &self.config.transforms {
            if transform.inputs.iter().any(|i| i == name) {
                output
                    .unbounded_send(fanout::ControlMessage::Add(
                        transform_name.clone(),
                        self.inputs[transform_name].get(),
                    ))
                    .unwrap();
            }
        }

        self.outputs.insert(name.to_string(), output);
    }

    fn setup_inputs(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let (tx, inputs) = new_pieces.inputs.remove(name).unwrap();

        for input in inputs {
            self.outputs[&input]
                .unbounded_send(fanout::ControlMessage::Add(name.to_string(), tx.get()))
                .unwrap();
        }

        self.inputs.insert(name.to_string(), tx);
    }

    fn replace_inputs(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let (tx, inputs) = new_pieces.inputs.remove(name).unwrap();

        let sink_inputs = self.config.sinks.get(name).map(|s| &s.inputs);
        let trans_inputs = self.config.transforms.get(name).map(|t| &t.inputs);
        let old_inputs = sink_inputs
            .or(trans_inputs)
            .unwrap()
            .iter()
            .collect::<HashSet<_>>();

        let new_inputs = inputs.iter().collect::<HashSet<_>>();

        let inputs_to_remove = &old_inputs - &new_inputs;
        let inputs_to_add = &new_inputs - &old_inputs;
        let inputs_to_replace = old_inputs.intersection(&new_inputs);

        for input in inputs_to_remove {
            if let Some(output) = self.outputs.get(input) {
                output
                    .unbounded_send(fanout::ControlMessage::Remove(name.to_string()))
                    .unwrap();
            }
        }

        for input in inputs_to_add {
            self.outputs[input]
                .unbounded_send(fanout::ControlMessage::Add(name.to_string(), tx.get()))
                .unwrap();
        }

        for &input in inputs_to_replace {
            self.outputs[input]
                .unbounded_send(fanout::ControlMessage::Replace(name.to_string(), tx.get()))
                .unwrap();
        }

        self.inputs.insert(name.to_string(), tx);
    }
}

fn to_remove_change_add<C>(
    old: &IndexMap<String, C>,
    new: &IndexMap<String, C>,
) -> (HashSet<String>, HashSet<String>, HashSet<String>)
where
    C: serde::Serialize + serde::Deserialize<'static>,
{
    let old_names = old.keys().cloned().collect::<HashSet<_>>();
    let new_names = new.keys().cloned().collect::<HashSet<_>>();

    let to_change = old_names
        .intersection(&new_names)
        .filter(|&n| {
            // This is a hack around the issue of comparing two
            // trait objects. Json is used here over toml since
            // toml does not support serializing `None`.
            let old_json = serde_json::to_vec(&old[n]).unwrap();
            let new_json = serde_json::to_vec(&new[n]).unwrap();
            old_json != new_json
        })
        .cloned()
        .collect::<HashSet<_>>();

    let to_remove = &old_names - &new_names;
    let to_add = &new_names - &old_names;

    (to_remove, to_change, to_add)
}

fn handle_errors(
    task: impl Future<Item = (), Error = ()>,
    abort_tx: mpsc::UnboundedSender<()>,
) -> impl Future<Item = (), Error = ()> {
    AssertUnwindSafe(task)
        .catch_unwind()
        .map_err(|_| ())
        .flatten()
        .or_else(move |()| {
            error!("Unhandled error");
            let _ = abort_tx.unbounded_send(());
            Err(())
        })
}

#[cfg(all(test, feature = "sinks-console", feature = "sources-socket"))]
mod tests {
    use crate::sinks::console::{ConsoleSinkConfig, Encoding, Target};
    use crate::sources::socket::SocketConfig;
    use crate::test_util::{next_addr, runtime};
    use crate::topology;
    use crate::topology::config::Config;

    #[test]
    fn topology_doesnt_reload_new_data_dir() {
        let mut rt = runtime();

        use std::path::Path;

        let mut old_config = Config::empty();
        old_config.add_source("in", SocketConfig::make_tcp_config(next_addr()));
        old_config.add_sink(
            "out",
            &[&"in"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );
        old_config.global.data_dir = Some(Path::new("/asdf").to_path_buf());
        let mut new_config = old_config.clone();

        let (mut topology, _crash) = topology::start(old_config, &mut rt, false).unwrap();

        new_config.global.data_dir = Some(Path::new("/qwerty").to_path_buf());

        topology.reload_config_and_respawn(new_config, &mut rt, false);

        assert_eq!(
            topology.config.global.data_dir,
            Some(Path::new("/asdf").to_path_buf())
        );
    }
}
