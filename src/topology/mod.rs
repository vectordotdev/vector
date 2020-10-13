//! Topology contains all topology based types.
//!
//! Topology is broken up into two main sections. The first
//! section contains all the main topology types include `Topology`
//! and the ability to start, stop and reload a config. The second
//! part contains config related items including config traits for
//! each type of component.

pub mod builder;
mod fanout;
mod task;

use crate::{
    buffers,
    config::{Config, ConfigDiff},
    shutdown::SourceShutdownCoordinator,
    topology::{builder::Pieces, task::Task},
};
use futures::{compat::Future01CompatExt, future, FutureExt, StreamExt, TryFutureExt};
use futures01::{sync::mpsc, Future};
use std::{
    collections::{HashMap, HashSet},
    panic::AssertUnwindSafe,
};
use tokio::time::{delay_until, interval, Duration, Instant};
use tracing_futures::Instrument;

// TODO: Result is only for compat, remove when not needed
type TaskHandle = tokio::task::JoinHandle<Result<(), ()>>;

#[allow(dead_code)]
pub struct RunningTopology {
    inputs: HashMap<String, buffers::BufferInputCloner>,
    outputs: HashMap<String, fanout::ControlChannel>,
    source_tasks: HashMap<String, TaskHandle>,
    tasks: HashMap<String, TaskHandle>,
    shutdown_coordinator: SourceShutdownCoordinator,
    config: Config,
    abort_tx: mpsc::UnboundedSender<()>,
}

pub async fn start_validated(
    config: Config,
    diff: ConfigDiff,
    mut pieces: Pieces,
    require_healthy: bool,
) -> Option<(RunningTopology, mpsc::UnboundedReceiver<()>)> {
    let (abort_tx, abort_rx) = mpsc::unbounded();

    let mut running_topology = RunningTopology {
        inputs: HashMap::new(),
        outputs: HashMap::new(),
        config,
        shutdown_coordinator: SourceShutdownCoordinator::default(),
        source_tasks: HashMap::new(),
        tasks: HashMap::new(),
        abort_tx,
    };

    if !running_topology
        .run_healthchecks(&diff, &mut pieces, require_healthy)
        .await
    {
        return None;
    }
    running_topology.connect_diff(&diff, &mut pieces);
    running_topology.spawn_diff(&diff, pieces);

    Some((running_topology, abort_rx))
}

pub async fn build_or_log_errors(config: &Config, diff: &ConfigDiff) -> Option<Pieces> {
    match builder::build_pieces(config, diff).await {
        Err(errors) => {
            for error in errors {
                error!("Configuration error: {}", error);
            }
            None
        }
        Ok(new_pieces) => Some(new_pieces),
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
    pub fn stop(self) -> impl Future<Item = (), Error = ()> {
        // Create handy handles collections of all tasks for the subsequent operations.
        let mut wait_handles = Vec::new();
        // We need a Vec here since source components have two tasks. One for pump in self.tasks,
        // and the other for source in self.source_tasks.
        let mut check_handles = HashMap::<String, Vec<_>>::new();

        // We need to give some time to the sources to gracefully shutdown, so we will merge
        // them with other tasks.
        for (name, task) in self.tasks.into_iter().chain(self.source_tasks.into_iter()) {
            let task = futures::compat::Compat::new(task)
                .map(|_result| ())
                .or_else(|_| futures01::future::ok(())) // Consider an errored task to be shutdown
                .shared();

            wait_handles.push(task.clone());
            check_handles.entry(name).or_default().push(task);
        }

        // If we reach this, we will forcefully shutdown the sources.
        let deadline = Instant::now() + Duration::from_secs(60);

        // If we reach the deadline, this future will print out which components won't
        // gracefully shutdown since we will start to forcefully shutdown the sources.
        let mut check_handles2 = check_handles.clone();
        let timeout = delay_until(deadline).map(move |_| {
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

            Ok(())
        });

        // Reports in intervals which components are still running.
        let reporter = interval(Duration::from_secs(5))
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
            .filter(|_| future::ready(false)) // Run indefinitely without emitting items
            .into_future()
            .map(|_| Ok(()));

        // Finishes once all tasks have shutdown.
        let success = futures01::future::join_all(wait_handles)
            .map(|_| ())
            .map_err(|_: futures01::future::SharedError<()>| ())
            .compat();

        // Aggregate future that ends once anything detects that all tasks have shutdown.
        let shutdown_complete_future = future::select_all(vec![
            Box::pin(timeout) as future::BoxFuture<'static, Result<(), ()>>,
            Box::pin(reporter) as future::BoxFuture<'static, Result<(), ()>>,
            Box::pin(success) as future::BoxFuture<'static, Result<(), ()>>,
        ])
        .map(|(result, _, _)| result.map(|_| ()).map_err(|_| ()))
        .compat();

        // Now kick off the shutdown process by shutting down the sources.
        let source_shutdown_complete = self.shutdown_coordinator.shutdown_all(deadline);

        source_shutdown_complete
            .join(shutdown_complete_future)
            .map(|_| ())
    }

    /// On Error, topology is in invalid state.
    /// May change componenets even if reload fails.
    pub async fn reload_config_and_respawn(
        &mut self,
        new_config: Config,
        require_healthy: bool,
    ) -> Result<bool, ()> {
        if self.config.global.data_dir != new_config.global.data_dir {
            error!("data_dir cannot be changed while reloading config file; reload aborted. Current value: {:?}", self.config.global.data_dir);
            return Ok(false);
        }

        let diff = ConfigDiff::new(&self.config, &new_config);

        // Checks passed so let's shutdown the difference.
        self.shutdown_diff(&diff).await;

        // Gives windows some time to make available any port
        // released by shutdown componenets.
        // Issue: https://github.com/timberio/vector/issues/3035
        if cfg!(windows) {
            // This value is guess work.
            tokio::time::delay_for(Duration::from_millis(200)).await;
        }

        // Now let's actually build the new pieces.
        if let Some(mut new_pieces) = build_or_log_errors(&new_config, &diff).await {
            if self
                .run_healthchecks(&diff, &mut new_pieces, require_healthy)
                .await
            {
                self.connect_diff(&diff, &mut new_pieces);
                self.spawn_diff(&diff, new_pieces);
                self.config = new_config;
                // We have successfully changed to new config.
                return Ok(true);
            }
        }

        // We need to rebuild the removed.
        info!("Rebuilding old configuration.");
        let diff = diff.flip();
        if let Some(mut new_pieces) = build_or_log_errors(&self.config, &diff).await {
            if self
                .run_healthchecks(&diff, &mut new_pieces, require_healthy)
                .await
            {
                self.connect_diff(&diff, &mut new_pieces);
                self.spawn_diff(&diff, new_pieces);
                // We have successfully returned to old config.
                return Ok(false);
            }
        }

        // We failed in rebuilding the old state.
        error!("Failed in rebuilding the old configuration.");

        Err(())
    }

    async fn run_healthchecks(
        &mut self,
        diff: &ConfigDiff,
        pieces: &mut Pieces,
        require_healthy: bool,
    ) -> bool {
        let healthchecks = take_healthchecks(diff, pieces)
            .into_iter()
            .map(|(_, task)| task);
        let healthchecks = future::try_join_all(healthchecks);

        info!("Running healthchecks.");
        if require_healthy {
            let success = healthchecks.await;

            if success.is_ok() {
                info!("All healthchecks passed.");
                true
            } else {
                error!("Sinks unhealthy.");
                false
            }
        } else {
            tokio::spawn(healthchecks);
            true
        }
    }

    /// Shutdowns removed and replaced pieces of topology.
    async fn shutdown_diff(&mut self, diff: &ConfigDiff) {
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

            let previous = self.tasks.remove(name).unwrap();
            drop(previous); // detach and forget

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

        futures01::future::join_all(source_shutdown_complete_futures)
            .compat()
            .await
            .unwrap();

        // Second pass now that all sources have shut down for final cleanup.
        for name in &diff.sources.to_remove {
            if let Some(task) = self.source_tasks.remove(name) {
                task.await.unwrap().unwrap();
            }
        }
        for name in &diff.sources.to_change {
            if let Some(task) = self.source_tasks.remove(name) {
                task.await.unwrap().unwrap();
            }
        }

        // Transforms
        for name in &diff.transforms.to_remove {
            info!("Removing transform {:?}", name);

            let previous = self.tasks.remove(name).unwrap();
            drop(previous); // detach and forget

            self.remove_inputs(&name);
            self.remove_outputs(&name);
        }

        // Sinks
        for name in &diff.sinks.to_remove {
            info!("Removing sink {:?}", name);

            let previous = self.tasks.remove(name).unwrap();
            drop(previous); // detach and forget

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
    fn spawn_diff(&mut self, diff: &ConfigDiff, mut new_pieces: Pieces) {
        // Sources
        for name in &diff.sources.to_change {
            info!("Rebuilding source {:?}", name);
            self.spawn_source(name, &mut new_pieces);
        }

        for name in &diff.sources.to_add {
            info!("Starting source {:?}", name);
            self.spawn_source(&name, &mut new_pieces);
        }

        // Transforms
        for name in &diff.transforms.to_change {
            info!("Rebuilding transform {:?}", name);
            self.spawn_transform(&name, &mut new_pieces);
        }

        for name in &diff.transforms.to_add {
            info!("Starting transform {:?}", name);
            self.spawn_transform(&name, &mut new_pieces);
        }

        // Sinks
        for name in &diff.sinks.to_change {
            info!("Rebuilding sink {:?}", name);
            self.spawn_sink(&name, &mut new_pieces);
        }

        for name in &diff.sinks.to_add {
            info!("Starting sink {:?}", name);
            self.spawn_sink(&name, &mut new_pieces);
        }
    }

    fn spawn_sink(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let task = new_pieces.tasks.remove(name).unwrap();
        let span = error_span!(
            "sink",
            component_kind = "sink",
            component_name = %task.name(),
            component_type = %task.typetag(),
        );
        let task = handle_errors(task.compat(), self.abort_tx.clone()).instrument(span);
        let spawned = tokio::spawn(task.compat());
        if let Some(previous) = self.tasks.insert(name.to_string(), spawned) {
            drop(previous); // detach and forget
        }
    }

    fn spawn_transform(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let task = new_pieces.tasks.remove(name).unwrap();
        let span = error_span!(
            "transform",
            component_kind = "transform",
            component_name = %task.name(),
            component_type = %task.typetag(),
        );
        let task = handle_errors(task.compat(), self.abort_tx.clone()).instrument(span);
        let spawned = tokio::spawn(task.compat());
        if let Some(previous) = self.tasks.insert(name.to_string(), spawned) {
            drop(previous); // detach and forget
        }
    }

    fn spawn_source(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let task = new_pieces.tasks.remove(name).unwrap();
        let span = error_span!(
            "source",
            component_kind = "source",
            component_name = %task.name(),
            component_type = %task.typetag(),
        );
        let task = handle_errors(task.compat(), self.abort_tx.clone()).instrument(span.clone());
        let spawned = tokio::spawn(task.compat());
        if let Some(previous) = self.tasks.insert(name.to_string(), spawned) {
            drop(previous); // detach and forget
        }

        self.shutdown_coordinator
            .takeover_source(name, &mut new_pieces.shutdown_coordinator);

        let source_task = new_pieces.source_tasks.remove(name).unwrap();
        let source_task =
            handle_errors(source_task.compat(), self.abort_tx.clone()).instrument(span);
        self.source_tasks
            .insert(name.to_string(), tokio::spawn(source_task.compat()));
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

    /// Borrows the Config
    pub fn config(&self) -> &Config {
        &self.config
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
    use crate::{
        config::Config,
        sinks::console::{ConsoleSinkConfig, Encoding, Target},
        sources::socket::SocketConfig,
        test_util::{next_addr, start_topology},
    };
    use std::path::Path;

    #[tokio::test]
    async fn topology_doesnt_reload_new_data_dir() {
        let mut old_config = Config::builder();
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

        let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;

        new_config.global.data_dir = Some(Path::new("/qwerty").to_path_buf());

        topology
            .reload_config_and_respawn(new_config.build().unwrap(), false)
            .await
            .unwrap();

        assert_eq!(
            topology.config.global.data_dir,
            Some(Path::new("/asdf").to_path_buf())
        );
    }
}

#[cfg(all(test, feature = "sinks-console", feature = "sources-splunk_hec"))]
mod reload_tests {
    use crate::config::Config;
    use crate::sinks::console::{ConsoleSinkConfig, Encoding, Target};
    use crate::sources::splunk_hec::SplunkConfig;
    use crate::test_util::{next_addr, start_topology};

    #[tokio::test]
    async fn topology_reuse_old_port() {
        let address = next_addr();

        let mut old_config = Config::builder();
        old_config.add_source("in1", SplunkConfig::on(address));
        old_config.add_sink(
            "out",
            &[&"in1"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );

        let mut new_config = Config::builder();
        new_config.add_source("in2", SplunkConfig::on(address));
        new_config.add_sink(
            "out",
            &[&"in2"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );

        let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
        assert!(topology
            .reload_config_and_respawn(new_config.build().unwrap(), false)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn topology_rebuild_old() {
        let address = next_addr();

        let mut old_config = Config::builder();
        old_config.add_source("in1", SplunkConfig::on(address));
        old_config.add_sink(
            "out",
            &[&"in1"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );

        let mut new_config = Config::builder();
        new_config.add_source("in1", SplunkConfig::on(address));
        new_config.add_source("in2", SplunkConfig::on(address));
        new_config.add_sink(
            "out",
            &[&"in1", &"in2"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );

        let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
        assert!(!topology
            .reload_config_and_respawn(new_config.build().unwrap(), false)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn topology_old() {
        let address = next_addr();

        let mut old_config = Config::builder();
        old_config.add_source("in1", SplunkConfig::on(address));
        old_config.add_sink(
            "out",
            &[&"in1"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );

        let (mut topology, _crash) =
            start_topology(old_config.clone().build().unwrap(), false).await;
        assert!(topology
            .reload_config_and_respawn(old_config.build().unwrap(), false)
            .await
            .unwrap());
    }
}

#[cfg(all(test, feature = "sinks-console", feature = "sources-generator"))]
mod source_finished_tests {
    use crate::{
        config::Config,
        sinks::console::{ConsoleSinkConfig, Encoding, Target},
        sources::generator::GeneratorConfig,
        test_util::start_topology,
    };
    use futures::compat::Future01CompatExt;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn sources_finished() {
        let mut old_config = Config::builder();
        let generator = GeneratorConfig::repeat(vec!["text".to_owned()], 1, None);
        old_config.add_source("in", generator);
        old_config.add_sink(
            "out",
            &[&"in"],
            ConsoleSinkConfig {
                target: Target::Stdout,
                encoding: Encoding::Text.into(),
            },
        );

        let (topology, _crash) = start_topology(old_config.build().unwrap(), false).await;

        timeout(Duration::from_secs(2), topology.sources_finished().compat())
            .await
            .unwrap()
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
    use crate::{
        config::{Config, DataType, GlobalOptions, SourceConfig},
        shutdown::ShutdownSignal,
        sinks::blackhole::BlackholeConfig,
        sources::stdin::StdinConfig,
        sources::Source,
        test_util::{start_topology, trace_init},
        transforms::json_parser::JsonParserConfig,
        Error, Pipeline,
    };
    use futures::compat::Future01CompatExt;
    use futures01::Future;
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

    #[async_trait::async_trait]
    #[typetag::serde(name = "mock")]
    impl SourceConfig for MockSourceConfig {
        async fn build(
            &self,
            _name: &str,
            _globals: &GlobalOptions,
            shutdown: ShutdownSignal,
            out: Pipeline,
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

    #[tokio::test]
    async fn closed_source() {
        let mut old_config = Config::builder();
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

        let mut new_config = Config::builder();
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

        let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;

        trigger_old.cancel();

        let finished = topology.sources_finished();
        finished.compat().await.unwrap();

        assert!(topology
            .reload_config_and_respawn(new_config.build().unwrap(), false)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn remove_sink() {
        trace_init();

        let mut old_config = Config::builder();
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

        let mut new_config = Config::builder();
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

        let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
        assert!(topology
            .reload_config_and_respawn(new_config.build().unwrap(), false)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn remove_transform() {
        trace_init();

        let mut old_config = Config::builder();
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

        let mut new_config = Config::builder();
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

        let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
        assert!(topology
            .reload_config_and_respawn(new_config.build().unwrap(), false)
            .await
            .unwrap());
    }
}
