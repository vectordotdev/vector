//! An interface into the system.

use std::env;

/// An interface between the test framework and external CLI commands and test
/// utilities.
#[derive(Debug)]
pub struct Interface {
    /// A command used to deploy a Helm chart into the kubernetes cluster and
    /// delete if from there.
    pub deploy_chart_command: String,

    /// A `kubectl` command used for generic cluster interaction.
    pub kubectl_command: String,
}

impl Interface {
    /// Create a new [`Interface`] instance with the parameters obtained from
    /// the process environment.
    pub fn from_env() -> Option<Self> {
        Some(Self {
            deploy_chart_command: env::var("KUBE_TEST_DEPLOY_COMMAND").ok()?,
            kubectl_command: env::var("VECTOR_TEST_KUBECTL")
                .unwrap_or_else(|_| "kubectl".to_owned()),
        })
    }
}
