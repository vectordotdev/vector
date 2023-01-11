#[cfg(feature = "enterprise")]
use futures_util::future::BoxFuture;

#[cfg(feature = "api")]
use crate::api;
#[cfg(feature = "enterprise")]
use crate::config::enterprise::{
    report_on_reload, EnterpriseError, EnterpriseMetadata, EnterpriseReporter,
};
use crate::internal_events::{
    VectorConfigLoadError, VectorRecoveryError, VectorReloadError, VectorReloaded,
};
use crate::{config, topology::RunningTopology};

pub struct TopologyController {
    pub topology: RunningTopology,
    pub config_paths: Vec<config::ConfigPath>,
    pub require_healthy: Option<bool>,
    #[cfg(feature = "enterprise")]
    pub enterprise_reporter: Option<EnterpriseReporter<BoxFuture<'static, ()>>>,
    #[cfg(feature = "api")]
    pub api_server: Option<api::Server>,
}

impl std::fmt::Debug for TopologyController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TopologyController")
            .field("config_paths", &self.config_paths)
            .field("require_healthy", &self.require_healthy)
            .finish()
    }
}

pub enum ReloadOutcome {
    NoConfig,
    MissingApiKey,
    Success,
    RolledBack,
    FatalError,
}

impl TopologyController {
    pub async fn reload(&mut self, new_config: Option<config::Config>) -> ReloadOutcome {
        use ReloadOutcome::*;

        if new_config.is_none() {
            emit!(VectorConfigLoadError);
            return NoConfig;
        }
        let mut new_config = new_config.unwrap();

        new_config
            .healthchecks
            .set_require_healthy(self.require_healthy);

        #[cfg(feature = "enterprise")]
        // Augment config to enable observability within Datadog, if applicable.
        match EnterpriseMetadata::try_from(&new_config) {
            Ok(metadata) => {
                if let Some(e) = report_on_reload(
                    &mut new_config,
                    metadata,
                    self.config_paths.clone(),
                    self.enterprise_reporter.as_ref(),
                ) {
                    self.enterprise_reporter = Some(e);
                }
            }
            Err(err) => {
                if let EnterpriseError::MissingApiKey = err {
                    emit!(VectorReloadError);
                    return MissingApiKey;
                }
            }
        }

        match self.topology.reload_config_and_respawn(new_config).await {
            Ok(true) => {
                #[cfg(feature = "api")]
                // Pass the new config to the API server.
                if let Some(ref api_server) = self.api_server {
                    api_server.update_config(self.topology.config());
                }

                emit!(VectorReloaded {
                    config_paths: &self.config_paths
                });
                Success
            }
            Ok(false) => {
                emit!(VectorReloadError);
                RolledBack
            }
            // Trigger graceful shutdown for what remains of the topology
            Err(()) => {
                emit!(VectorReloadError);
                emit!(VectorRecoveryError);
                FatalError
            }
        }
    }

    pub async fn stop(self) {
        self.topology.stop().await;
    }
}
