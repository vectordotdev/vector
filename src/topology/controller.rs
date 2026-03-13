use std::sync::Arc;

use futures_util::FutureExt as _;
use tokio::sync::{Mutex, MutexGuard};

#[cfg(feature = "api")]
use crate::api;
use crate::{
    config,
    extra_context::ExtraContext,
    internal_events::{VectorRecoveryError, VectorReloadError, VectorReloaded},
    signal::ShutdownError,
    topology::{ReloadError, RunningTopology},
};

#[derive(Clone, Debug)]
pub struct SharedTopologyController(Arc<Mutex<TopologyController>>);

impl SharedTopologyController {
    pub fn new(inner: TopologyController) -> Self {
        Self(Arc::new(Mutex::new(inner)))
    }

    pub fn blocking_lock(&self) -> MutexGuard<'_, TopologyController> {
        self.0.blocking_lock()
    }

    pub async fn lock(&self) -> MutexGuard<'_, TopologyController> {
        self.0.lock().await
    }

    pub fn try_into_inner(self) -> Result<Mutex<TopologyController>, Self> {
        Arc::try_unwrap(self.0).map_err(Self)
    }
}

pub struct TopologyController {
    pub topology: RunningTopology,
    pub config_paths: Vec<config::ConfigPath>,
    pub require_healthy: Option<bool>,
    #[cfg(feature = "api")]
    pub api_server: Option<api::Server>,
    pub extra_context: ExtraContext,
}

impl std::fmt::Debug for TopologyController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TopologyController")
            .field("config_paths", &self.config_paths)
            .field("require_healthy", &self.require_healthy)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub enum ReloadOutcome {
    MissingApiKey,
    Success,
    RolledBack,
    FatalError(ShutdownError),
}

impl TopologyController {
    pub async fn reload(&mut self, mut new_config: config::Config) -> ReloadOutcome {
        new_config
            .healthchecks
            .set_require_healthy(self.require_healthy);

        // Start the api server or disable it, if necessary
        #[cfg(feature = "api")]
        if !new_config.api.enabled {
            if let Some(server) = self.api_server.take() {
                debug!("Dropping api server.");
                drop(server)
            }
        } else if self.api_server.is_none() {
            use std::sync::atomic::AtomicBool;

            use tokio::runtime::Handle;

            use crate::internal_events::ApiStarted;

            debug!("Starting api server.");

            self.api_server = match api::Server::start(
                self.topology.config(),
                self.topology.watch(),
                Arc::<AtomicBool>::clone(&self.topology.running),
                &Handle::current(),
            ) {
                Ok(api_server) => {
                    emit!(ApiStarted {
                        addr: new_config.api.address.unwrap(),
                        playground: new_config.api.playground,
                        graphql: new_config.api.graphql,
                    });

                    Some(api_server)
                }
                Err(error) => {
                    let error = error.to_string();
                    error!("An error occurred that Vector couldn't handle: {}.", error);
                    return ReloadOutcome::FatalError(ShutdownError::ApiFailed { error });
                }
            }
        }

        match self
            .topology
            .reload_config_and_respawn(new_config, self.extra_context.clone())
            .await
        {
            Ok(()) => {
                #[cfg(feature = "api")]
                // Pass the new config to the API server.
                if let Some(ref api_server) = self.api_server {
                    api_server.update_config(self.topology.config());
                }

                emit!(VectorReloaded {
                    config_paths: &self.config_paths
                });
                ReloadOutcome::Success
            }
            Err(ReloadError::GlobalOptionsChanged { changed_fields }) => {
                error!(
                    message = "Config reload rejected due to non-reloadable global options.",
                    changed_fields = %changed_fields.join(", "),
                    internal_log_rate_limit = false,
                );
                emit!(VectorReloadError {
                    reason: "global_options_changed",
                });
                ReloadOutcome::RolledBack
            }
            Err(ReloadError::GlobalDiffFailed { source }) => {
                error!(
                    message = "Config reload rejected because computing global diff failed.",
                    error = %source,
                    internal_log_rate_limit = false,
                );
                emit!(VectorReloadError {
                    reason: "global_diff_failed",
                });
                ReloadOutcome::RolledBack
            }
            Err(ReloadError::TopologyBuildFailed) => {
                emit!(VectorReloadError {
                    reason: "topology_build_failed",
                });
                ReloadOutcome::RolledBack
            }
            Err(ReloadError::FailedToRestore) => {
                emit!(VectorReloadError {
                    reason: "restore_failed",
                });
                emit!(VectorRecoveryError);
                ReloadOutcome::FatalError(ShutdownError::ReloadFailedToRestore)
            }
        }
    }

    pub async fn stop(self) {
        self.topology.stop().await;
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
    pub async fn sources_finished(mutex: SharedTopologyController) {
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
}
