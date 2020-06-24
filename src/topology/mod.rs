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

use crate::topology::{builder::Pieces, task::Task};

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
    let diff = ConfigDiff::initial(&config);
    validate(&config, &diff, rt.executor())
        .and_then(|pieces| start_validated(config, diff, pieces, rt, require_healthy))
}

pub fn start_validated(
    config: Config,
    diff: ConfigDiff,
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

    if !running_topology.run_healthchecks(&diff, &mut pieces, rt, require_healthy) {
        return None;
    }
    running_topology.connect_diff(&diff, &mut pieces);
    running_topology.spawn_diff(&diff, pieces, rt);
    running_topology.config = config;

    Some((running_topology, abort_rx))
}

pub fn validate(config: &Config, diff: &ConfigDiff, exec: runtime::TaskExecutor) -> Option<Pieces> {
    match builder::check_build(config, diff, exec) {
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

pub fn take_healthchecks(diff: &ConfigDiff, pieces: &mut Pieces) -> Vec<(String, Task)> {
    (&diff.sinks.to_change | &diff.sinks.to_add)
        .into_iter()
        .filter_map(|name| {
            pieces
                .healthchecks
                .remove(&name)
                .map(move |task| (name, task))
        })
        .collect()
}

impl RunningTopology {
    /// Returned future will finish once all current sources have finished.
    pub fn sources_finished(&self) -> impl Future<Item = (), Error = ()> {
        self.shutdown_coordinator.shutdown_tripwire()
    }

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
        // Create handy handles collections of all tasks for the subsequent operations.
        let mut wait_handles = Vec::new();
        // We need a Vec here since source compnents have two tasks. One for pump in self.tasks,
        // and the other for source in self.source_tasks.
        let mut check_handles = HashMap::<String, Vec<_>>::new();

        // We need to give some time to the sources to gracefully shutdown, so we will merge
        // them with other tasks.
        for (name, task) in self.tasks.into_iter().chain(self.source_tasks.into_iter()) {
            let task = task
                .or_else(|_| future::ok(())) // Consider an errored task to be shutdown
                .shared();

            wait_handles.push(task.clone());
            check_handles.entry(name).or_default().push(task);
        }

        // If we reach this, we will forcefully shutdown the sources.
        let deadline = Instant::now() + Duration::from_secs(60);

        // If we reach the deadline, this future will print out which components won't
        // gracefully shutdown since we will start to forcefully shutdown the sources.
        let mut check_handles2 = check_handles.clone();
        let timeout = timer::Delay::new(deadline)
            .map(move |_| {
                // Remove all tasks that have shutdown.
                check_handles2.retain(|_name, handles| {
                    retain(handles, |handle| {
                        handle.poll().map(|p| p.is_not_ready()).unwrap_or(false)
                    });
                    !handles.is_empty()
                });
                let remaining_components = check_handles2.keys().cloned().collect::<Vec<_>>();

                error!(
                    "Failed to gracefully shut down in time. Killing: {}",
                    remaining_components.join(", ")
                );
            })
            .map_err(|err| panic!("Timer error: {:?}", err));

        // Reports in intervals which componenets are still running.
        let reporter = timer::Interval::new_interval(Duration::from_secs(5))
            .inspect(move |_| {
                // Remove all tasks that have shutdown.
                check_handles.retain(|_name, handles| {
                    retain(handles, |handle| {
                        handle.poll().map(|p| p.is_not_ready()).unwrap_or(false)
                    });
                    !handles.is_empty()
                });
                let remaining_components = check_handles.keys().cloned().collect::<Vec<_>>();

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

        // Finishes once all tasks have shutdown.
        let success = future::join_all(wait_handles)
            .map(|_| ())
            .map_err(|_: future::SharedError<()>| ());

        // Aggregate future that ends once anything detectes that all tasks have shutdown.
        let shutdown_complete_future =
            future::select_all::<Vec<Box<dyn Future<Item = (), Error = ()> + Send>>>(vec![
                Box::new(timeout),
                Box::new(reporter),
                Box::new(success),
            ])
            .map(|_| ())
            .map_err(|_| ());

        // Now kick off the shutdown process by shutting down the sources.
        let source_shutdown_complete = self.shutdown_coordinator.shutdown_all(deadline);

        source_shutdown_complete
            .join(shutdown_complete_future)
            .map(|_| ())
    }

    /// On Error, topology is in invalid state.
    pub fn reload_config_and_respawn(
        &mut self,
        new_config: Config,
        rt: &mut runtime::Runtime,
        require_healthy: bool,
    ) -> Result<bool, ()> {
        if self.config.global.data_dir != new_config.global.data_dir {
            error!("data_dir cannot be changed while reloading config file; reload aborted. Current value: {:?}", self.config.global.data_dir);
            return Ok(false);
        }

        if let Err(errors) = builder::check(&new_config) {
            for error in errors {
                error!("Configuration error: {}", error);
            }
            return Ok(false);
        }

        let diff = ConfigDiff::new(&self.config, &new_config);

        // Checks passed so let's shutdown the difference.
        self.shutdown_diff(&diff, rt);

        // Now let's actually build the new pieces.
        if let Some(mut new_pieces) = validate(&new_config, &diff, rt.executor()) {
            if self.run_healthchecks(&diff, &mut new_pieces, rt, require_healthy) {
                self.connect_diff(&diff, &mut new_pieces);
                self.spawn_diff(&diff, new_pieces, rt);
                self.config = new_config;
                // We have succesfully changed to new config.
                return Ok(true);
            }
        }

        // We need to rebuild the removed.
        info!("Rebuilding old configuration.");
        let diff = diff.flip();
        if let Some(mut new_pieces) = validate(&self.config, &diff, rt.executor()) {
            if self.run_healthchecks(&diff, &mut new_pieces, rt, require_healthy) {
                self.connect_diff(&diff, &mut new_pieces);
                self.spawn_diff(&diff, new_pieces, rt);
                // We have succesfully returned to old config.
                return Ok(false);
            }
        }

        // We failed in rebuilding the old state.
        error!("Failed in rebuilding the old configuration.");

        Err(())
    }

    fn run_healthchecks(
        &mut self,
        diff: &ConfigDiff,
        pieces: &mut Pieces,
        rt: &mut runtime::Runtime,
        require_healthy: bool,
    ) -> bool {
        let healthchecks = take_healthchecks(diff, pieces)
            .into_iter()
            .map(|(_, task)| task);
        let healthchecks = futures01::future::join_all(healthchecks).map(|_| ());

        info!("Running healthchecks.");
        if require_healthy {
            let jh = rt.spawn_handle_std(healthchecks.compat());
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

    /// Shutdowns removed and replaced pieces of topology.
    fn shutdown_diff(&mut self, diff: &ConfigDiff, rt: &mut runtime::Runtime) {
        // Sources
        let timeout = Duration::from_secs(30); //sec

        // First pass to tell the sources to shut down.
        let mut source_shutdown_complete_futures = Vec::new();

        // Only log that we are waiting for shutdown if we are actually removing
        // sources.
        if !diff.sources.to_remove.is_empty() {
            info!(
                "Waiting for up to {} seconds for sources to finish shutting down",
                timeout.as_secs()
            );
        }

        let deadline = Instant::now() + timeout;
        for name in &diff.sources.to_remove {
            info!("Removing source {:?}", name);

            self.tasks.remove(name).unwrap().forget();

            self.remove_outputs(name);
            source_shutdown_complete_futures
                .push(self.shutdown_coordinator.shutdown_source(name, deadline));
        }
        for name in &diff.sources.to_change {
            self.remove_outputs(name);
            source_shutdown_complete_futures
                .push(self.shutdown_coordinator.shutdown_source(name, deadline));
        }

        // Wait for the shutdowns to complete

        // Only log message if there are actual futures to check.
        if !source_shutdown_complete_futures.is_empty() {
            info!(
                "Waiting for up to {} seconds for sources to finish shutting down",
                timeout.as_secs()
            );
        }

        rt.block_on(future::join_all(source_shutdown_complete_futures))
            .unwrap();

        // Second pass now that all sources have shut down for final cleanup.
        for name in &diff.sources.to_remove {
            self.source_tasks.remove(name).wait().unwrap();
        }
        for name in &diff.sources.to_change {
            self.source_tasks.remove(name).wait().unwrap();
        }

        // Transforms
        for name in &diff.transforms.to_remove {
            info!("Removing transform {:?}", name);

            self.tasks.remove(name).unwrap().forget();

            self.remove_inputs(&name);
            self.remove_outputs(&name);
        }

        // Sinks
        for name in &diff.sinks.to_remove {
            info!("Removing sink {:?}", name);

            self.tasks.remove(name).unwrap().forget();

            self.remove_inputs(&name);
        }
    }

    /// Rewires topology
    fn connect_diff(&mut self, diff: &ConfigDiff, new_pieces: &mut Pieces) {
        // Sources
        for name in diff.sources.changed_and_added() {
            self.setup_outputs(&name, new_pieces);
        }

        // Transforms
        // Make sure all transform outputs are set up before another transform might try use
        // it as an input
        for name in diff.transforms.changed_and_added() {
            self.setup_outputs(&name, new_pieces);
        }

        for name in &diff.transforms.to_change {
            self.replace_inputs(&name, new_pieces);
        }

        for name in &diff.transforms.to_add {
            self.setup_inputs(&name, new_pieces);
        }

        // Sinks
        for name in &diff.sinks.to_change {
            self.replace_inputs(&name, new_pieces);
        }

        for name in &diff.sinks.to_add {
            self.setup_inputs(&name, new_pieces);
        }
    }

    /// Starts new and changed pieces of topology.
    fn spawn_diff(&mut self, diff: &ConfigDiff, mut new_pieces: Pieces, rt: &mut runtime::Runtime) {
        // Sources
        for name in &diff.sources.to_change {
            info!("Rebuilding source {:?}", name);
            self.spawn_source(name, &mut new_pieces, rt);
        }

        for name in &diff.sources.to_add {
            info!("Starting source {:?}", name);
            self.spawn_source(&name, &mut new_pieces, rt);
        }

        // Transforms
        for name in &diff.transforms.to_change {
            info!("Rebuilding transform {:?}", name);
            self.spawn_transform(&name, &mut new_pieces, rt);
        }

        for name in &diff.transforms.to_add {
            info!("Starting transform {:?}", name);
            self.spawn_transform(&name, &mut new_pieces, rt);
        }

        // Sinks
        for name in &diff.sinks.to_change {
            info!("Rebuilding sink {:?}", name);
            self.spawn_sink(&name, &mut new_pieces, rt);
        }

        for name in &diff.sinks.to_add {
            info!("Starting sink {:?}", name);
            self.spawn_sink(&name, &mut new_pieces, rt);
        }
    }

    fn spawn_sink(
        &mut self,
        name: &str,
        new_pieces: &mut builder::Pieces,
        rt: &mut runtime::Runtime,
    ) {
        let task = new_pieces.tasks.remove(name).unwrap();
        let span = info_span!("sink", name = %task.name(), r#type = %task.typetag());
        let task = handle_errors(task, self.abort_tx.clone()).instrument(span);
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
        let task = handle_errors(task, self.abort_tx.clone()).instrument(span);
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
        let task = handle_errors(task, self.abort_tx.clone()).instrument(span.clone());
        let spawned = oneshot::spawn(task, &rt.executor());
        if let Some(previous) = self.tasks.insert(name.to_string(), spawned) {
            previous.forget();
        }

        self.shutdown_coordinator
            .takeover_source(name, &mut new_pieces.shutdown_coordinator);

        let source_task = new_pieces.source_tasks.remove(name).unwrap();
        let source_task = handle_errors(source_task, self.abort_tx.clone()).instrument(span);
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
                    // This can only fail if we are disconnected, which is a valid situation.
                    let _ = output.unbounded_send(fanout::ControlMessage::Remove(name.to_string()));
                }
            }
        }
    }

    fn setup_outputs(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let output = new_pieces.outputs.remove(name).unwrap();

        for (sink_name, sink) in &self.config.sinks {
            if sink.inputs.iter().any(|i| i == name) {
                // Sink may have been removed with the new config so it may not be present.
                if let Some(input) = self.inputs.get(sink_name) {
                    output
                        .unbounded_send(fanout::ControlMessage::Add(sink_name.clone(), input.get()))
                        .expect("Components shouldn't be spawned before connecting them together.");
                }
            }
        }
        for (transform_name, transform) in &self.config.transforms {
            if transform.inputs.iter().any(|i| i == name) {
                // Transform may have been removed with the new config so it may not be present.
                if let Some(input) = self.inputs.get(transform_name) {
                    output
                        .unbounded_send(fanout::ControlMessage::Add(
                            transform_name.clone(),
                            input.get(),
                        ))
                        .expect("Components shouldn't be spawned before connecting them together.");
                }
            }
        }

        self.outputs.insert(name.to_string(), output);
    }

    fn setup_inputs(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let (tx, inputs) = new_pieces.inputs.remove(name).unwrap();

        for input in inputs {
            // This can only fail if we are disconnected, which is a valid situation.
            let _ = self.outputs[&input]
                .unbounded_send(fanout::ControlMessage::Add(name.to_string(), tx.get()));
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
                // This can only fail if we are disconnected, which is a valid situation.
                let _ = output.unbounded_send(fanout::ControlMessage::Remove(name.to_string()));
            }
        }

        for input in inputs_to_add {
            // This can only fail if we are disconnected, which is a valid situation.
            let _ = self.outputs[input]
                .unbounded_send(fanout::ControlMessage::Add(name.to_string(), tx.get()));
        }

        for &input in inputs_to_replace {
            // This can only fail if we are disconnected, which is a valid situation.
            let _ = self.outputs[input]
                .unbounded_send(fanout::ControlMessage::Replace(name.to_string(), tx.get()));
        }

        self.inputs.insert(name.to_string(), tx);
    }
}

pub struct ConfigDiff {
    sources: Difference,
    transforms: Difference,
    sinks: Difference,
}

impl ConfigDiff {
    pub fn initial(initial: &Config) -> Self {
        Self::new(&Config::empty(), initial)
    }

    fn new(old: &Config, new: &Config) -> Self {
        ConfigDiff {
            sources: Difference::new(&old.sources, &new.sources),
            transforms: Difference::new(&old.transforms, &new.transforms),
            sinks: Difference::new(&old.sinks, &new.sinks),
        }
    }

    /// Swaps removed with added in Differences.
    fn flip(mut self) -> Self {
        self.sources.flip();
        self.transforms.flip();
        self.sinks.flip();
        self
    }
}

struct Difference {
    to_remove: HashSet<String>,
    to_change: HashSet<String>,
    to_add: HashSet<String>,
}

impl Difference {
    fn new<C>(old: &IndexMap<String, C>, new: &IndexMap<String, C>) -> Self
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

        Self {
            to_remove,
            to_change,
            to_add,
        }
    }

    /// True if name is present in new config and either not in the old one or is different.
    fn contains_new(&self, name: &str) -> bool {
        self.to_add.contains(name) || self.to_change.contains(name)
    }

    fn flip(&mut self) {
        std::mem::swap(&mut self.to_remove, &mut self.to_add);
    }

    fn changed_and_added(&self) -> impl Iterator<Item = &String> {
        self.to_change.iter().chain(self.to_add.iter())
    }
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
            error!("An error occurred that vector couldn't handle.");
            let _ = abort_tx.unbounded_send(());
            Err(())
        })
}

/// If the closure returns false, then the element is removed
fn retain<T>(vec: &mut Vec<T>, mut retain_filter: impl FnMut(&mut T) -> bool) {
    let mut i = 0;
    while let Some(data) = vec.get_mut(i) {
        if retain_filter(data) {
            i += 1;
        } else {
            let _ = vec.remove(i);
        }
    }
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

        let _ = topology.reload_config_and_respawn(new_config, &mut rt, false);

        assert_eq!(
            topology.config.global.data_dir,
            Some(Path::new("/asdf").to_path_buf())
        );
    }
}

#[cfg(all(test, feature = "sinks-console", feature = "sources-splunk_hec"))]
mod reload_tests {
    use crate::sinks::console::{ConsoleSinkConfig, Encoding, Target};
    use crate::sources::splunk_hec::SplunkConfig;
    use crate::test_util::{next_addr, runtime};
    use crate::topology;
    use crate::topology::config::Config;

    #[test]
    fn topology_reuse_old_port() {
        let address = next_addr();

        let mut rt = runtime();

        let mut old_config = Config::empty();
        old_config.add_source("in1", SplunkConfig::on(address));
        old_config.add_sink(
            "out",
            &[&"in1"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );

        let mut new_config = Config::empty();
        new_config.add_source("in2", SplunkConfig::on(address));
        new_config.add_sink(
            "out",
            &[&"in2"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );

        let (mut topology, _crash) = topology::start(old_config, &mut rt, false).unwrap();

        assert!(topology
            .reload_config_and_respawn(new_config, &mut rt, false)
            .unwrap());
    }

    #[test]
    fn topology_rebuild_old() {
        let address = next_addr();

        let mut rt = runtime();

        let mut old_config = Config::empty();
        old_config.add_source("in1", SplunkConfig::on(address));
        old_config.add_sink(
            "out",
            &[&"in1"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );

        let mut new_config = Config::empty();
        old_config.add_source("in1", SplunkConfig::on(address));
        new_config.add_source("in2", SplunkConfig::on(address));
        new_config.add_sink(
            "out",
            &[&"in1", &"in2"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );

        let (mut topology, _crash) = topology::start(old_config, &mut rt, false).unwrap();

        assert!(!topology
            .reload_config_and_respawn(new_config, &mut rt, false)
            .unwrap());
    }

    #[test]
    fn topology_old() {
        let address = next_addr();

        let mut rt = runtime();

        let mut old_config = Config::empty();
        old_config.add_source("in1", SplunkConfig::on(address));
        old_config.add_sink(
            "out",
            &[&"in1"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );

        let (mut topology, _crash) = topology::start(old_config.clone(), &mut rt, false).unwrap();

        assert!(topology
            .reload_config_and_respawn(old_config, &mut rt, false)
            .unwrap());
    }
}

#[cfg(all(test, feature = "sinks-console", feature = "sources-generator"))]
mod source_finished_tests {
    use crate::sinks::console::{ConsoleSinkConfig, Encoding, Target};
    use crate::sources::generator::GeneratorConfig;
    use crate::test_util::runtime;
    use crate::topology;
    use crate::topology::config::Config;
    use std::time::Duration;
    use tokio01::util::FutureExt;

    #[test]
    fn sources_finished() {
        let mut rt = runtime();

        let mut old_config = Config::empty();
        old_config.add_source("in", GeneratorConfig::repeat(vec!["text".to_owned()], 1));
        old_config.add_sink(
            "out",
            &[&"in"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );

        let (topology, _crash) = topology::start(old_config.clone(), &mut rt, false).unwrap();

        rt.block_on(topology.sources_finished().timeout(Duration::from_secs(2)))
            .unwrap();
    }
}

#[cfg(all(
    test,
    feature = "sinks-blackhole",
    feature = "sources-stdin",
    feature = "transforms-json_parser"
))]
mod transient_state_tests {
    use crate::event::Event;
    use crate::shutdown::ShutdownSignal;
    use crate::sinks::blackhole::BlackholeConfig;
    use crate::sources::stdin::StdinConfig;
    use crate::sources::Source;
    use crate::test_util::runtime;
    use crate::topology::config::{Config, DataType, GlobalOptions, SourceConfig};
    use crate::transforms::json_parser::JsonParserConfig;
    use crate::{topology, Error};
    use futures01::{sync::mpsc::Sender, Future};
    use serde::{Deserialize, Serialize};
    use stream_cancel::{Trigger, Tripwire};

    #[derive(Debug, Deserialize, Serialize)]
    pub struct MockSourceConfig {
        #[serde(skip)]
        tripwire: Option<Tripwire>,
    }

    impl MockSourceConfig {
        pub fn new() -> (Trigger, Self) {
            let (trigger, tripwire) = Tripwire::new();
            (
                trigger,
                Self {
                    tripwire: Some(tripwire),
                },
            )
        }
    }

    #[typetag::serde(name = "mock")]
    impl SourceConfig for MockSourceConfig {
        fn build(
            &self,
            _name: &str,
            _globals: &GlobalOptions,
            shutdown: ShutdownSignal,
            out: Sender<Event>,
        ) -> Result<Source, Error> {
            let source = shutdown
                .map(|_| ())
                .select(self.tripwire.clone().unwrap())
                .map(|_| std::mem::drop(out))
                .map_err(|_| ());
            Ok(Box::new(source))
        }

        fn output_type(&self) -> DataType {
            DataType::Log
        }

        fn source_type(&self) -> &'static str {
            "mock"
        }
    }

    #[test]
    fn closed_source() {
        let mut rt = runtime();

        let mut old_config = Config::empty();
        let (trigger_old, source) = MockSourceConfig::new();
        old_config.add_source("in", source);
        old_config.add_transform(
            "trans",
            &["in"],
            JsonParserConfig {
                drop_field: true,
                ..JsonParserConfig::default()
            },
        );
        old_config.add_sink("out1", &["trans"], BlackholeConfig { print_amount: 1000 });
        old_config.add_sink("out2", &["trans"], BlackholeConfig { print_amount: 1000 });

        let mut new_config = Config::empty();
        let (_trigger_new, source) = MockSourceConfig::new();
        new_config.add_source("in", source);
        new_config.add_transform(
            "trans",
            &["in"],
            JsonParserConfig {
                drop_field: false,
                ..JsonParserConfig::default()
            },
        );
        new_config.add_sink("out1", &["trans"], BlackholeConfig { print_amount: 1000 });

        let (mut topology, _crash) = topology::start(old_config, &mut rt, false).unwrap();

        trigger_old.cancel();

        rt.block_on(topology.sources_finished()).unwrap();

        assert!(topology
            .reload_config_and_respawn(new_config, &mut rt, false)
            .unwrap());
    }

    #[test]
    fn remove_sink() {
        crate::test_util::trace_init();
        let mut rt = runtime();

        let mut old_config = Config::empty();
        old_config.add_source("in", StdinConfig::default());
        old_config.add_transform(
            "trans",
            &["in"],
            JsonParserConfig {
                drop_field: true,
                ..JsonParserConfig::default()
            },
        );
        old_config.add_sink("out1", &["trans"], BlackholeConfig { print_amount: 1000 });
        old_config.add_sink("out2", &["trans"], BlackholeConfig { print_amount: 1000 });

        let mut new_config = Config::empty();
        new_config.add_source("in", StdinConfig::default());
        new_config.add_transform(
            "trans",
            &["in"],
            JsonParserConfig {
                drop_field: false,
                ..JsonParserConfig::default()
            },
        );
        new_config.add_sink("out1", &["trans"], BlackholeConfig { print_amount: 1000 });

        let (mut topology, _crash) = topology::start(old_config, &mut rt, false).unwrap();

        assert!(topology
            .reload_config_and_respawn(new_config, &mut rt, false)
            .unwrap());
    }

    #[test]
    fn remove_transform() {
        crate::test_util::trace_init();
        let mut rt = runtime();

        let mut old_config = Config::empty();
        old_config.add_source("in", StdinConfig::default());
        old_config.add_transform(
            "trans1",
            &["in"],
            JsonParserConfig {
                drop_field: true,
                ..JsonParserConfig::default()
            },
        );
        old_config.add_transform(
            "trans2",
            &["trans1"],
            JsonParserConfig {
                drop_field: true,
                ..JsonParserConfig::default()
            },
        );
        old_config.add_sink("out1", &["trans1"], BlackholeConfig { print_amount: 1000 });
        old_config.add_sink("out2", &["trans2"], BlackholeConfig { print_amount: 1000 });

        let mut new_config = Config::empty();
        new_config.add_source("in", StdinConfig::default());
        new_config.add_transform(
            "trans1",
            &["in"],
            JsonParserConfig {
                drop_field: false,
                ..JsonParserConfig::default()
            },
        );
        new_config.add_sink("out1", &["trans1"], BlackholeConfig { print_amount: 1000 });

        let (mut topology, _crash) = topology::start(old_config, &mut rt, false).unwrap();

        assert!(topology
            .reload_config_and_respawn(new_config, &mut rt, false)
            .unwrap());
    }
}
