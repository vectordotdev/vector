use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use futures::{future, Future, FutureExt};
use tokio::{
    sync::{mpsc, watch},
    time::{interval, sleep_until, Duration, Instant},
};
use tracing::Instrument;
use vector_lib::buffers::topology::channel::BufferSender;
use vector_lib::trigger::DisabledTrigger;

use super::{
    builder,
    builder::TopologyPieces,
    fanout::{ControlChannel, ControlMessage},
    handle_errors, retain, take_healthchecks,
    task::TaskOutput,
    BuiltBuffer, TapOutput, TapResource, TaskHandle, WatchRx, WatchTx,
};
use crate::{
    config::{ComponentKey, Config, ConfigDiff, HealthcheckOptions, Inputs, OutputId, Resource},
    event::EventArray,
    extra_context::ExtraContext,
    shutdown::SourceShutdownCoordinator,
    signal::ShutdownError,
    spawn_named,
};

pub type ShutdownErrorReceiver = mpsc::UnboundedReceiver<ShutdownError>;

#[allow(dead_code)]
pub struct RunningTopology {
    inputs: HashMap<ComponentKey, BufferSender<EventArray>>,
    inputs_tap_metadata: HashMap<ComponentKey, Inputs<OutputId>>,
    outputs: HashMap<OutputId, ControlChannel>,
    outputs_tap_metadata: HashMap<ComponentKey, (&'static str, String)>,
    source_tasks: HashMap<ComponentKey, TaskHandle>,
    tasks: HashMap<ComponentKey, TaskHandle>,
    shutdown_coordinator: SourceShutdownCoordinator,
    detach_triggers: HashMap<ComponentKey, DisabledTrigger>,
    pub(crate) config: Config,
    pub(crate) abort_tx: mpsc::UnboundedSender<ShutdownError>,
    watch: (WatchTx, WatchRx),
    pub(crate) running: Arc<AtomicBool>,
    graceful_shutdown_duration: Option<Duration>,
}

impl RunningTopology {
    pub fn new(config: Config, abort_tx: mpsc::UnboundedSender<ShutdownError>) -> Self {
        Self {
            inputs: HashMap::new(),
            inputs_tap_metadata: HashMap::new(),
            outputs: HashMap::new(),
            outputs_tap_metadata: HashMap::new(),
            shutdown_coordinator: SourceShutdownCoordinator::default(),
            detach_triggers: HashMap::new(),
            source_tasks: HashMap::new(),
            tasks: HashMap::new(),
            abort_tx,
            watch: watch::channel(TapResource::default()),
            running: Arc::new(AtomicBool::new(true)),
            graceful_shutdown_duration: config.graceful_shutdown_duration,
            config,
        }
    }

    /// Gets the configuration that represents this running topology.
    pub const fn config(&self) -> &Config {
        &self.config
    }

    /// Creates a subscription to topology changes.
    ///
    /// This is used by the tap API to observe configuration changes, and re-wire tap sinks.
    pub fn watch(&self) -> watch::Receiver<TapResource> {
        self.watch.1.clone()
    }

    /// Signal that all sources in this topology are ended.
    ///
    /// The future returned by this function will finish once all the sources in
    /// this topology have finished. This allows the caller to wait for or
    /// detect that the sources in the topology are no longer
    /// producing. [`Application`][crate::app::Application], as an example, uses this as a
    /// shutdown signal.
    pub fn sources_finished(&self) -> future::BoxFuture<'static, ()> {
        self.shutdown_coordinator.shutdown_tripwire()
    }

    /// Shut down all topology components.
    ///
    /// This function sends the shutdown signal to all sources in this topology
    /// and returns a future that resolves once all components (sources,
    /// transforms, and sinks) have finished shutting down. Transforms and sinks
    /// will shut down automatically once their input tasks finish.
    ///
    /// This function takes ownership of `self`, so once it returns everything
    /// in the [`RunningTopology`] instance has been dropped except for the
    /// `tasks` map. This map gets moved into the returned future and is used to
    /// poll for when the tasks have completed. Once the returned future is
    /// dropped then everything from this RunningTopology instance is fully
    /// dropped.
    pub fn stop(self) -> impl Future<Output = ()> {
        // Update the API's health endpoint to signal shutdown
        self.running.store(false, Ordering::Relaxed);
        // Create handy handles collections of all tasks for the subsequent
        // operations.
        let mut wait_handles = Vec::new();
        // We need a Vec here since source components have two tasks. One for
        // pump in self.tasks, and the other for source in self.source_tasks.
        let mut check_handles = HashMap::<ComponentKey, Vec<_>>::new();

        // We need to give some time to the sources to gracefully shutdown, so
        // we will merge them with other tasks.
        for (key, task) in self.tasks.into_iter().chain(self.source_tasks.into_iter()) {
            let task = task.map(|_result| ()).shared();

            wait_handles.push(task.clone());
            check_handles.entry(key).or_default().push(task);
        }

        // If we reach this, we will forcefully shutdown the sources. If None, we will never force shutdown.
        let deadline = self
            .graceful_shutdown_duration
            .map(|grace_period| Instant::now() + grace_period);

        let timeout = if let Some(deadline) = deadline {
            // If we reach the deadline, this future will print out which components
            // won't gracefully shutdown since we will start to forcefully shutdown
            // the sources.
            let mut check_handles2 = check_handles.clone();
            Box::pin(async move {
                sleep_until(deadline).await;
                // Remove all tasks that have shutdown.
                check_handles2.retain(|_key, handles| {
                    retain(handles, |handle| handle.peek().is_none());
                    !handles.is_empty()
                });
                let remaining_components = check_handles2
                    .keys()
                    .map(|item| item.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");

                error!(
                    components = ?remaining_components,
                    "Failed to gracefully shut down in time. Killing components."
                );
            }) as future::BoxFuture<'static, ()>
        } else {
            Box::pin(future::pending()) as future::BoxFuture<'static, ()>
        };

        // Reports in intervals which components are still running.
        let mut interval = interval(Duration::from_secs(5));
        let reporter = async move {
            loop {
                interval.tick().await;

                // Remove all tasks that have shutdown.
                check_handles.retain(|_key, handles| {
                    retain(handles, |handle| handle.peek().is_none());
                    !handles.is_empty()
                });
                let remaining_components = check_handles
                    .keys()
                    .map(|item| item.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");

                let time_remaining = deadline
                    .map(|d| match d.checked_duration_since(Instant::now()) {
                        Some(remaining) => format!("{} seconds left", remaining.as_secs()),
                        None => "overdue".to_string(),
                    })
                    .unwrap_or("no time limit".to_string());

                info!(
                    remaining_components = ?remaining_components,
                    time_remaining = ?time_remaining,
                    "Shutting down... Waiting on running components."
                );
            }
        };

        // Finishes once all tasks have shutdown.
        let success = futures::future::join_all(wait_handles).map(|_| ());

        // Aggregate future that ends once anything detects that all tasks have shutdown.
        let shutdown_complete_future = future::select_all(vec![
            Box::pin(timeout) as future::BoxFuture<'static, ()>,
            Box::pin(reporter) as future::BoxFuture<'static, ()>,
            Box::pin(success) as future::BoxFuture<'static, ()>,
        ]);

        // Now kick off the shutdown process by shutting down the sources.
        let source_shutdown_complete = self.shutdown_coordinator.shutdown_all(deadline);

        futures::future::join(source_shutdown_complete, shutdown_complete_future).map(|_| ())
    }

    /// Attempts to load a new configuration and update this running topology.
    ///
    /// If the new configuration was valid, and all changes were able to be made -- removing of
    /// old components, changing of existing components, adding of new components -- then `Ok(true)`
    /// is returned.
    ///
    /// If the new configuration is not valid, or not all of the changes in the new configuration
    /// were able to be made, then this method will attempt to undo the changes made and bring the
    /// topology back to its previous state.  If either of these scenarios occur, then `Ok(false)`
    /// is returned.
    ///
    /// # Errors
    ///
    /// If all changes from the new configuration cannot be made, and the current configuration
    /// cannot be fully restored, then `Err(())` is returned.
    pub async fn reload_config_and_respawn(
        &mut self,
        new_config: Config,
        extra_context: ExtraContext,
    ) -> Result<bool, ()> {
        info!("Reloading running topology with new configuration.");

        if self.config.global != new_config.global {
            error!(
                message =
                "Global options can't be changed while reloading config file; reload aborted. Please restart Vector to reload the configuration file."
            );
            return Ok(false);
        }

        // Calculate the change between the current configuration and the new configuration, and
        // shutdown any components that are changing so that we can reclaim their buffers before
        // spawning the new version of the component.
        //
        // We also shutdown any component that is simply being removed entirely.
        let diff = ConfigDiff::new(&self.config, &new_config);
        let buffers = self.shutdown_diff(&diff, &new_config).await;

        // Gives windows some time to make available any port
        // released by shutdown components.
        // Issue: https://github.com/vectordotdev/vector/issues/3035
        if cfg!(windows) {
            // This value is guess work.
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        // Try to build all of the new components coming from the new configuration.  If we can
        // successfully build them, we'll attempt to connect them up to the topology and spawn their
        // respective component tasks.
        if let Some(mut new_pieces) = TopologyPieces::build_or_log_errors(
            &new_config,
            &diff,
            buffers.clone(),
            extra_context.clone(),
        )
        .await
        {
            // If healthchecks are configured for any of the changing/new components, try running
            // them before moving forward with connecting and spawning.  In some cases, healthchecks
            // failing may be configured as a non-blocking issue and so we'll still continue on.
            if self
                .run_healthchecks(&diff, &mut new_pieces, new_config.healthchecks)
                .await
            {
                self.connect_diff(&diff, &mut new_pieces).await;
                self.spawn_diff(&diff, new_pieces);
                self.config = new_config;

                info!("New configuration loaded successfully.");

                return Ok(true);
            }
        }

        // We failed to build, connect, and spawn all of the changed/new components, so we flip
        // around the configuration differential to generate all the components that we need to
        // bring back to restore the current configuration.
        warn!("Failed to completely load new configuration. Restoring old configuration.");

        let diff = diff.flip();
        if let Some(mut new_pieces) =
            TopologyPieces::build_or_log_errors(&self.config, &diff, buffers, extra_context.clone())
                .await
        {
            if self
                .run_healthchecks(&diff, &mut new_pieces, self.config.healthchecks)
                .await
            {
                self.connect_diff(&diff, &mut new_pieces).await;
                self.spawn_diff(&diff, new_pieces);

                info!("Old configuration restored successfully.");

                return Ok(false);
            }
        }

        error!("Failed to restore old configuration.");

        Err(())
    }

    pub(crate) async fn run_healthchecks(
        &mut self,
        diff: &ConfigDiff,
        pieces: &mut TopologyPieces,
        options: HealthcheckOptions,
    ) -> bool {
        if options.enabled {
            let healthchecks = take_healthchecks(diff, pieces)
                .into_iter()
                .map(|(_, task)| task);
            let healthchecks = future::try_join_all(healthchecks);

            info!("Running healthchecks.");
            if options.require_healthy {
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
        } else {
            true
        }
    }

    /// Shuts down any changed/removed component in the given configuration diff.
    ///
    /// If buffers for any of the changed/removed components can be recovered, they'll be returned.
    async fn shutdown_diff(
        &mut self,
        diff: &ConfigDiff,
        new_config: &Config,
    ) -> HashMap<ComponentKey, BuiltBuffer> {
        // First, we shutdown any changed/removed sources. This ensures that we can allow downstream
        // components to terminate naturally by virtue of the flow of events stopping.
        if diff.sources.any_changed_or_removed() {
            let timeout = Duration::from_secs(30);
            let mut source_shutdown_handles = Vec::new();

            let deadline = Instant::now() + timeout;
            for key in &diff.sources.to_remove {
                debug!(component = %key, "Removing source.");

                let previous = self.tasks.remove(key).unwrap();
                drop(previous); // detach and forget

                self.remove_outputs(key);
                source_shutdown_handles
                    .push(self.shutdown_coordinator.shutdown_source(key, deadline));
            }

            for key in &diff.sources.to_change {
                debug!(component = %key, "Changing source.");

                self.remove_outputs(key);
                source_shutdown_handles
                    .push(self.shutdown_coordinator.shutdown_source(key, deadline));
            }

            debug!(
                "Waiting for up to {} seconds for source(s) to finish shutting down.",
                timeout.as_secs()
            );
            futures::future::join_all(source_shutdown_handles).await;

            // Final cleanup pass now that all changed/removed sources have signalled as having shutdown.
            for key in diff.sources.removed_and_changed() {
                if let Some(task) = self.source_tasks.remove(key) {
                    task.await.unwrap().unwrap();
                }
            }
        }

        // Next, we shutdown any changed/removed transforms.  Same as before: we want allow
        // downstream components to terminate naturally by virtue of the flow of events stopping.
        //
        // Since transforms are entirely driven by the flow of events into them from upstream
        // components, the shutdown of sources they depend on, or the shutdown of transforms they
        // depend on, and thus the closing of their buffer, will naturally cause them to shutdown,
        // which is why we don't do any manual triggering of shutdown here.
        for key in &diff.transforms.to_remove {
            debug!(component = %key, "Removing transform.");

            let previous = self.tasks.remove(key).unwrap();
            drop(previous); // detach and forget

            self.remove_inputs(key, diff, new_config).await;
            self.remove_outputs(key);
        }

        for key in &diff.transforms.to_change {
            debug!(component = %key, "Changing transform.");

            self.remove_inputs(key, diff, new_config).await;
            self.remove_outputs(key);
        }

        // Now we'll process any changed/removed sinks.
        //
        // At this point both the old and the new config don't have conflicts in their resource
        // usage. So if we combine their resources, all found conflicts are between to be removed
        // and to be added components.
        let remove_sink = diff
            .sinks
            .removed_and_changed()
            .map(|key| (key, self.config.sink(key).unwrap().resources(key)));
        let add_source = diff
            .sources
            .changed_and_added()
            .map(|key| (key, new_config.source(key).unwrap().inner.resources()));
        let add_sink = diff
            .sinks
            .changed_and_added()
            .map(|key| (key, new_config.sink(key).unwrap().resources(key)));
        let conflicts = Resource::conflicts(
            remove_sink.map(|(key, value)| ((true, key), value)).chain(
                add_sink
                    .chain(add_source)
                    .map(|(key, value)| ((false, key), value)),
            ),
        )
        .into_iter()
        .flat_map(|(_, components)| components)
        .collect::<HashSet<_>>();
        // Existing conflicting sinks
        let conflicting_sinks = conflicts
            .into_iter()
            .filter(|&(existing_sink, _)| existing_sink)
            .map(|(_, key)| key.clone());

        // For any sink whose buffer configuration didn't change, we can reuse their buffer.
        let reuse_buffers = diff
            .sinks
            .to_change
            .iter()
            .filter(|&key| {
                self.config.sink(key).unwrap().buffer == new_config.sink(key).unwrap().buffer
            })
            .cloned()
            .collect::<HashSet<_>>();

        // For any existing sink that has a conflicting resource dependency with a changed/added
        // sink, or for any sink that we want to reuse their buffer, we need to explicit wait for
        // them to finish processing so we can reclaim ownership of those resources/buffers.
        let wait_for_sinks = conflicting_sinks
            .chain(reuse_buffers.iter().cloned())
            .collect::<HashSet<_>>();

        // First, we remove any inputs to removed sinks so they can naturally shut down.
        for key in &diff.sinks.to_remove {
            debug!(component = %key, "Removing sink.");
            self.remove_inputs(key, diff, new_config).await;
        }

        // After that, for any changed sinks, we temporarily detach their inputs (not remove) so
        // they can naturally shutdown and allow us to recover their buffers if possible.
        let mut buffer_tx = HashMap::new();

        for key in &diff.sinks.to_change {
            debug!(component = %key, "Changing sink.");
            if reuse_buffers.contains(key) {
                self.detach_triggers
                    .remove(key)
                    .unwrap()
                    .into_inner()
                    .cancel();

                // We explicitly clone the input side of the buffer and store it so we don't lose
                // it when we remove the inputs below.
                //
                // We clone instead of removing here because otherwise the input will be missing for
                // the rest of the reload process, which violates the assumption that all previous
                // inputs for components not being removed are still available. It's simpler to
                // allow the "old" input to stick around and be replaced (even though that's
                // basically a no-op since we're reusing the same buffer) than it is to pass around
                // info about which sinks are having their buffers reused and treat them differently
                // at other stages.
                buffer_tx.insert(key.clone(), self.inputs.get(key).unwrap().clone());
            }
            self.remove_inputs(key, diff, new_config).await;
        }

        // Now that we've disconnected or temporarily detached the inputs to all changed/removed
        // sinks, we can actually wait for them to shutdown before collecting any buffers that are
        // marked for reuse.
        //
        // If a sink we're removing isn't tying up any resource that a changed/added sink depends
        // on, we don't bother waiting for it to shutdown.
        for key in &diff.sinks.to_remove {
            let previous = self.tasks.remove(key).unwrap();
            if wait_for_sinks.contains(key) {
                debug!(message = "Waiting for sink to shutdown.", %key);
                previous.await.unwrap().unwrap();
            } else {
                drop(previous); // detach and forget
            }
        }

        let mut buffers = HashMap::<ComponentKey, BuiltBuffer>::new();
        for key in &diff.sinks.to_change {
            if wait_for_sinks.contains(key) {
                let previous = self.tasks.remove(key).unwrap();
                debug!(message = "Waiting for sink to shutdown.", %key);
                let buffer = previous.await.unwrap().unwrap();

                if reuse_buffers.contains(key) {
                    // We clone instead of removing here because otherwise the input will be
                    // missing for the rest of the reload process, which violates the assumption
                    // that all previous inputs for components not being removed are still
                    // available. It's simpler to allow the "old" input to stick around and be
                    // replaced (even though that's basically a no-op since we're reusing the same
                    // buffer) than it is to pass around info about which sinks are having their
                    // buffers reused and treat them differently at other stages.
                    let tx = buffer_tx.remove(key).unwrap();
                    let rx = match buffer {
                        TaskOutput::Sink(rx) => rx.into_inner(),
                        _ => unreachable!(),
                    };

                    buffers.insert(key.clone(), (tx, Arc::new(Mutex::new(Some(rx)))));
                }
            }
        }

        buffers
    }

    /// Connects all changed/added components in the given configuration diff.
    pub(crate) async fn connect_diff(
        &mut self,
        diff: &ConfigDiff,
        new_pieces: &mut TopologyPieces,
    ) {
        debug!("Connecting changed/added component(s).");

        // Update tap metadata
        if !self.watch.0.is_closed() {
            for key in &diff.sources.to_remove {
                // Sources only have outputs
                self.outputs_tap_metadata.remove(key);
            }

            for key in &diff.transforms.to_remove {
                // Transforms can have both inputs and outputs
                self.outputs_tap_metadata.remove(key);
                self.inputs_tap_metadata.remove(key);
            }

            for key in &diff.sinks.to_remove {
                // Sinks only have inputs
                self.inputs_tap_metadata.remove(key);
            }

            for key in diff.sources.changed_and_added() {
                if let Some(task) = new_pieces.tasks.get(key) {
                    self.outputs_tap_metadata
                        .insert(key.clone(), ("source", task.typetag().to_string()));
                }
            }

            for key in diff.transforms.changed_and_added() {
                if let Some(task) = new_pieces.tasks.get(key) {
                    self.outputs_tap_metadata
                        .insert(key.clone(), ("transform", task.typetag().to_string()));
                }
            }

            for (key, input) in &new_pieces.inputs {
                self.inputs_tap_metadata
                    .insert(key.clone(), input.1.clone());
            }
        }

        // We configure the outputs of any changed/added sources first, so they're available to any
        // transforms and sinks that come afterwards.
        for key in diff.sources.changed_and_added() {
            debug!(component = %key, "Configuring outputs for source.");
            self.setup_outputs(key, new_pieces).await;
        }

        // We configure the outputs of any changed/added transforms next, for the same reason: we
        // need them to be available to any transforms and sinks that come afterwards.
        for key in diff.transforms.changed_and_added() {
            debug!(component = %key, "Configuring outputs for transform.");
            self.setup_outputs(key, new_pieces).await;
        }

        // Now that all possible outputs are configured, we can start wiring up inputs, starting
        // with transforms.
        for key in diff.transforms.changed_and_added() {
            debug!(component = %key, "Connecting inputs for transform.");
            self.setup_inputs(key, diff, new_pieces).await;
        }

        // Now that all sources and transforms are fully configured, we can wire up sinks.
        for key in diff.sinks.changed_and_added() {
            debug!(component = %key, "Connecting inputs for sink.");
            self.setup_inputs(key, diff, new_pieces).await;
        }

        // We do a final pass here to reconnect unchanged components.
        //
        // Why would we reconnect unchanged components?  Well, as sources and transforms will
        // recreate their fanouts every time they're changed, we can run into a situation where a
        // transform/sink, which we'll call B, is pointed at a source/transform that was changed, which
        // we'll call A, but because B itself didn't change at all, we haven't yet reconnected it.
        //
        // Instead of propagating connections forward -- B reconnecting A forcefully -- we only
        // connect components backwards i.e. transforms to sources/transforms, and sinks to
        // sources/transforms, to ensure we're connecting components in order.
        self.reattach_severed_inputs(diff);

        // Broadcast any topology changes to subscribers.
        if !self.watch.0.is_closed() {
            let outputs = self
                .outputs
                .clone()
                .into_iter()
                .flat_map(|(output_id, control_tx)| {
                    self.outputs_tap_metadata.get(&output_id.component).map(
                        |(component_kind, component_type)| {
                            (
                                TapOutput {
                                    output_id,
                                    component_kind,
                                    component_type: component_type.clone(),
                                },
                                control_tx,
                            )
                        },
                    )
                })
                .collect::<HashMap<_, _>>();

            let mut removals = diff.sources.to_remove.clone();
            removals.extend(diff.transforms.to_remove.iter().cloned());
            self.watch
                .0
                .send(TapResource {
                    outputs,
                    inputs: self.inputs_tap_metadata.clone(),
                    source_keys: diff
                        .sources
                        .changed_and_added()
                        .map(|key| key.to_string())
                        .collect(),
                    sink_keys: diff
                        .sinks
                        .changed_and_added()
                        .map(|key| key.to_string())
                        .collect(),
                    // Note, only sources and transforms are relevant. Sinks do
                    // not have outputs to tap.
                    removals,
                })
                .expect("Couldn't broadcast config changes.");
        }
    }

    async fn setup_outputs(
        &mut self,
        key: &ComponentKey,
        new_pieces: &mut builder::TopologyPieces,
    ) {
        let outputs = new_pieces.outputs.remove(key).unwrap();
        for (port, output) in outputs {
            debug!(component = %key, output_id = ?port, "Configuring output for component.");

            let id = OutputId {
                component: key.clone(),
                port,
            };

            self.outputs.insert(id, output);
        }
    }

    async fn setup_inputs(
        &mut self,
        key: &ComponentKey,
        diff: &ConfigDiff,
        new_pieces: &mut builder::TopologyPieces,
    ) {
        let (tx, inputs) = new_pieces.inputs.remove(key).unwrap();

        let old_inputs = self
            .config
            .inputs_for_node(key)
            .into_iter()
            .flatten()
            .cloned()
            .collect::<HashSet<_>>();

        let new_inputs = inputs.iter().cloned().collect::<HashSet<_>>();
        let inputs_to_add = &new_inputs - &old_inputs;

        for input in inputs {
            let output = self.outputs.get_mut(&input).expect("unknown output");

            if diff.contains(&input.component) || inputs_to_add.contains(&input) {
                // If the input we're connecting to is changing, that means its outputs will have been
                // recreated, so instead of replacing a paused sink, we have to add it to this new
                // output for the first time, since there's nothing to actually replace at this point.
                debug!(component = %key, fanout_id = %input, "Adding component input to fanout.");

                _ = output.send(ControlMessage::Add(key.clone(), tx.clone()));
            } else {
                // We know that if this component is connected to a given input, and neither
                // components were changed, then the output must still exist, which means we paused
                // this component's connection to its output, so we have to replace that connection
                // now:
                debug!(component = %key, fanout_id = %input, "Replacing component input in fanout.");

                _ = output.send(ControlMessage::Replace(key.clone(), tx.clone()));
            }
        }

        self.inputs.insert(key.clone(), tx);
        new_pieces
            .detach_triggers
            .remove(key)
            .map(|trigger| self.detach_triggers.insert(key.clone(), trigger.into()));
    }

    fn remove_outputs(&mut self, key: &ComponentKey) {
        self.outputs.retain(|id, _output| &id.component != key);
    }

    async fn remove_inputs(&mut self, key: &ComponentKey, diff: &ConfigDiff, new_config: &Config) {
        self.inputs.remove(key);
        self.detach_triggers.remove(key);

        let old_inputs = self.config.inputs_for_node(key).expect("node exists");
        let new_inputs = new_config
            .inputs_for_node(key)
            .unwrap_or_default()
            .iter()
            .collect::<HashSet<_>>();

        for input in old_inputs {
            if let Some(output) = self.outputs.get_mut(input) {
                if diff.contains(&input.component)
                    || diff.is_removed(key)
                    || !new_inputs.contains(input)
                {
                    // 3 cases to remove the input:
                    //
                    // Case 1: If the input we're removing ourselves from is changing, that means its
                    // outputs will be recreated, so instead of pausing the sink, we just delete it
                    // outright to ensure things are clean.
                    //
                    // Case 2: If this component itself is being removed, then pausing makes no sense
                    // because it isn't coming back.
                    //
                    // Case 3: This component is no longer connected to the input from new config.
                    debug!(component = %key, fanout_id = %input, "Removing component input from fanout.");

                    _ = output.send(ControlMessage::Remove(key.clone()));
                } else {
                    // We know that if this component is connected to a given input, and it isn't being
                    // changed, then it will exist when we reconnect inputs, so we should pause it
                    // now to pause further sends through that component until we reconnect:
                    debug!(component = %key, fanout_id = %input, "Pausing component input in fanout.");

                    _ = output.send(ControlMessage::Pause(key.clone()));
                }
            }
        }
    }

    fn reattach_severed_inputs(&mut self, diff: &ConfigDiff) {
        let unchanged_transforms = self
            .config
            .transforms()
            .filter(|(key, _)| !diff.transforms.contains(key));
        for (transform_key, transform) in unchanged_transforms {
            let changed_outputs = get_changed_outputs(diff, transform.inputs.clone());
            for output_id in changed_outputs {
                debug!(component = %transform_key, fanout_id = %output_id.component, "Reattaching component input to fanout.");

                let input = self.inputs.get(transform_key).cloned().unwrap();
                let output = self.outputs.get_mut(&output_id).unwrap();
                _ = output.send(ControlMessage::Add(transform_key.clone(), input));
            }
        }

        let unchanged_sinks = self
            .config
            .sinks()
            .filter(|(key, _)| !diff.sinks.contains(key));
        for (sink_key, sink) in unchanged_sinks {
            let changed_outputs = get_changed_outputs(diff, sink.inputs.clone());
            for output_id in changed_outputs {
                debug!(component = %sink_key, fanout_id = %output_id.component, "Reattaching component input to fanout.");

                let input = self.inputs.get(sink_key).cloned().unwrap();
                let output = self.outputs.get_mut(&output_id).unwrap();
                _ = output.send(ControlMessage::Add(sink_key.clone(), input));
            }
        }
    }

    /// Starts any new or changed components in the given configuration diff.
    pub(crate) fn spawn_diff(&mut self, diff: &ConfigDiff, mut new_pieces: TopologyPieces) {
        for key in &diff.sources.to_change {
            debug!(message = "Spawning changed source.", key = %key);
            self.spawn_source(key, &mut new_pieces);
        }

        for key in &diff.sources.to_add {
            debug!(message = "Spawning new source.", key = %key);
            self.spawn_source(key, &mut new_pieces);
        }

        for key in &diff.transforms.to_change {
            debug!(message = "Spawning changed transform.", key = %key);
            self.spawn_transform(key, &mut new_pieces);
        }

        for key in &diff.transforms.to_add {
            debug!(message = "Spawning new transform.", key = %key);
            self.spawn_transform(key, &mut new_pieces);
        }

        for key in &diff.sinks.to_change {
            debug!(message = "Spawning changed sink.", key = %key);
            self.spawn_sink(key, &mut new_pieces);
        }

        for key in &diff.sinks.to_add {
            trace!(message = "Spawning new sink.", key = %key);
            self.spawn_sink(key, &mut new_pieces);
        }
    }

    fn spawn_sink(&mut self, key: &ComponentKey, new_pieces: &mut builder::TopologyPieces) {
        let task = new_pieces.tasks.remove(key).unwrap();
        let span = error_span!(
            "sink",
            component_kind = "sink",
            component_id = %task.id(),
            component_type = %task.typetag(),
        );

        let task_span = span.or_current();
        #[cfg(feature = "allocation-tracing")]
        if crate::internal_telemetry::allocations::is_allocation_tracing_enabled() {
            let group_id = crate::internal_telemetry::allocations::acquire_allocation_group_id(
                task.id().to_string(),
                "sink".to_string(),
                task.typetag().to_string(),
            );
            debug!(
                component_kind = "sink",
                component_type = task.typetag(),
                component_id = task.id(),
                group_id = group_id.as_raw().to_string(),
                "Registered new allocation group."
            );
            group_id.attach_to_span(&task_span);
        }

        let task_name = format!(">> {} ({})", task.typetag(), task.id());
        let task = {
            let key = key.clone();
            handle_errors(task, self.abort_tx.clone(), |error| {
                ShutdownError::SinkAborted { key, error }
            })
        }
        .instrument(task_span);
        let spawned = spawn_named(task, task_name.as_ref());
        if let Some(previous) = self.tasks.insert(key.clone(), spawned) {
            drop(previous); // detach and forget
        }
    }

    fn spawn_transform(&mut self, key: &ComponentKey, new_pieces: &mut builder::TopologyPieces) {
        let task = new_pieces.tasks.remove(key).unwrap();
        let span = error_span!(
            "transform",
            component_kind = "transform",
            component_id = %task.id(),
            component_type = %task.typetag(),
        );

        let task_span = span.or_current();
        #[cfg(feature = "allocation-tracing")]
        if crate::internal_telemetry::allocations::is_allocation_tracing_enabled() {
            let group_id = crate::internal_telemetry::allocations::acquire_allocation_group_id(
                task.id().to_string(),
                "transform".to_string(),
                task.typetag().to_string(),
            );
            debug!(
                component_kind = "transform",
                component_type = task.typetag(),
                component_id = task.id(),
                group_id = group_id.as_raw().to_string(),
                "Registered new allocation group."
            );
            group_id.attach_to_span(&task_span);
        }

        let task_name = format!(">> {} ({}) >>", task.typetag(), task.id());
        let task = {
            let key = key.clone();
            handle_errors(task, self.abort_tx.clone(), |error| {
                ShutdownError::TransformAborted { key, error }
            })
        }
        .instrument(task_span);
        let spawned = spawn_named(task, task_name.as_ref());
        if let Some(previous) = self.tasks.insert(key.clone(), spawned) {
            drop(previous); // detach and forget
        }
    }

    fn spawn_source(&mut self, key: &ComponentKey, new_pieces: &mut builder::TopologyPieces) {
        let task = new_pieces.tasks.remove(key).unwrap();
        let span = error_span!(
            "source",
            component_kind = "source",
            component_id = %task.id(),
            component_type = %task.typetag(),
        );

        let task_span = span.or_current();
        #[cfg(feature = "allocation-tracing")]
        if crate::internal_telemetry::allocations::is_allocation_tracing_enabled() {
            let group_id = crate::internal_telemetry::allocations::acquire_allocation_group_id(
                task.id().to_string(),
                "source".to_string(),
                task.typetag().to_string(),
            );

            debug!(
                component_kind = "source",
                component_type = task.typetag(),
                component_id = task.id(),
                group_id = group_id.as_raw().to_string(),
                "Registered new allocation group."
            );
            group_id.attach_to_span(&task_span);
        }

        let task_name = format!("{} ({}) >>", task.typetag(), task.id());
        let task = {
            let key = key.clone();
            handle_errors(task, self.abort_tx.clone(), |error| {
                ShutdownError::SourceAborted { key, error }
            })
        }
        .instrument(task_span.clone());
        let spawned = spawn_named(task, task_name.as_ref());
        if let Some(previous) = self.tasks.insert(key.clone(), spawned) {
            drop(previous); // detach and forget
        }

        self.shutdown_coordinator
            .takeover_source(key, &mut new_pieces.shutdown_coordinator);

        // Now spawn the actual source task.
        let source_task = new_pieces.source_tasks.remove(key).unwrap();
        let source_task = {
            let key = key.clone();
            handle_errors(source_task, self.abort_tx.clone(), |error| {
                ShutdownError::SourceAborted { key, error }
            })
        }
        .instrument(task_span);
        self.source_tasks
            .insert(key.clone(), spawn_named(source_task, task_name.as_ref()));
    }

    pub async fn start_init_validated(
        config: Config,
        extra_context: ExtraContext,
    ) -> Option<(Self, ShutdownErrorReceiver)> {
        let diff = ConfigDiff::initial(&config);
        let pieces =
            TopologyPieces::build_or_log_errors(&config, &diff, HashMap::new(), extra_context)
                .await?;
        Self::start_validated(config, diff, pieces).await
    }

    pub async fn start_validated(
        config: Config,
        diff: ConfigDiff,
        mut pieces: TopologyPieces,
    ) -> Option<(Self, ShutdownErrorReceiver)> {
        let (abort_tx, abort_rx) = mpsc::unbounded_channel();

        let expire_metrics = match (
            config.global.expire_metrics,
            config.global.expire_metrics_secs,
        ) {
            (Some(e), None) => {
                warn!(
                "DEPRECATED: `expire_metrics` setting is deprecated and will be removed in a future version. Use `expire_metrics_secs` instead."
            );
                Some(e.as_secs_f64())
            }
            (Some(_), Some(_)) => {
                error!("Cannot set both `expire_metrics` and `expire_metrics_secs`.");
                return None;
            }
            (None, e) => e,
        };

        if let Err(error) = crate::metrics::Controller::get()
            .expect("Metrics must be initialized")
            .set_expiry(expire_metrics)
        {
            error!(message = "Invalid metrics expiry.", %error);
            return None;
        }

        let mut running_topology = Self::new(config, abort_tx);

        if !running_topology
            .run_healthchecks(&diff, &mut pieces, running_topology.config.healthchecks)
            .await
        {
            return None;
        }
        running_topology.connect_diff(&diff, &mut pieces).await;
        running_topology.spawn_diff(&diff, pieces);

        Some((running_topology, abort_rx))
    }
}

fn get_changed_outputs(diff: &ConfigDiff, output_ids: Inputs<OutputId>) -> Vec<OutputId> {
    let mut changed_outputs = Vec::new();

    for source_key in &diff.sources.to_change {
        changed_outputs.extend(
            output_ids
                .iter()
                .filter(|id| &id.component == source_key)
                .cloned(),
        );
    }

    for transform_key in &diff.transforms.to_change {
        changed_outputs.extend(
            output_ids
                .iter()
                .filter(|id| &id.component == transform_key)
                .cloned(),
        );
    }

    changed_outputs
}
