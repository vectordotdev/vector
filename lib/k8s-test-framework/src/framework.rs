//! The test framework main entry point.

use super::{
    exec_tail, log_lookup, namespace, test_pod, up_down, vector, wait_for_resource,
    wait_for_rollout, Interface, Reader, Result,
};

/// Framework wraps the interface to the system with an easy-to-use rust API
/// optimized for implementing test cases.
#[derive(Debug)]
pub struct Framework {
    interface: Interface,
}

impl Framework {
    /// Create a new [`Framework`] powered by the passed interface.
    pub fn new(interface: Interface) -> Self {
        Self { interface }
    }

    /// Deploy `vector` into a cluster.
    pub async fn vector(
        &self,
        namespace: &str,
        helm_chart: &str,
        config: vector::Config<'_>,
    ) -> Result<up_down::Manager<vector::CommandBuilder>> {
        let mut manager = vector::manager(
            self.interface.deploy_vector_command.as_str(),
            namespace,
            helm_chart,
            config,
        )?;
        manager.up().await?;
        Ok(manager)
    }

    /// Create a new namespace.
    pub async fn namespace(
        &self,
        namespace: &str,
    ) -> Result<up_down::Manager<namespace::CommandBuilder>> {
        let mut manager = namespace::manager(&self.interface.kubectl_command, namespace);
        manager.up().await?;
        Ok(manager)
    }

    /// Create a new test `Pod`.
    pub async fn test_pod(
        &self,
        config: test_pod::Config,
    ) -> Result<up_down::Manager<test_pod::CommandBuilder>> {
        let mut manager = test_pod::manager(&self.interface.kubectl_command, config);
        manager.up().await?;
        Ok(manager)
    }

    /// Initialize log lookup for a particular `resource` in a particular
    /// `namespace`.
    pub fn logs(&self, namespace: &str, resource: &str) -> Result<Reader> {
        log_lookup(&self.interface.kubectl_command, namespace, resource)
    }

    /// Exec a `tail -f` command reading the specified `file` within
    /// a `Container` in a `Pod` of a specified `resource` at the specified
    /// `namespace`.
    pub fn exec_tail(&self, namespace: &str, resource: &str, file: &str) -> Result<Reader> {
        exec_tail(&self.interface.kubectl_command, namespace, resource, file)
    }

    /// Wait for a set of `resources` in a specified `namespace` to achieve
    /// `wait_for` state.
    /// Use `extra` to pass additional arguments to `kubectl`.
    pub async fn wait<'a>(
        &self,
        namespace: &str,
        resources: impl IntoIterator<Item = &'a str>,
        wait_for: wait_for_resource::WaitFor<&'_ str>,
        extra: impl IntoIterator<Item = &'a str>,
    ) -> Result<()> {
        wait_for_resource::namespace(
            &self.interface.kubectl_command,
            namespace,
            resources,
            wait_for,
            extra,
        )
        .await
    }

    /// Wait for a set of `resources` in any namespace to achieve `wait_for`
    /// state.
    /// Use `extra` to pass additional arguments to `kubectl`.
    pub async fn wait_all_namespaces<'a>(
        &self,
        resources: impl IntoIterator<Item = &'a str>,
        wait_for: wait_for_resource::WaitFor<&'_ str>,
        extra: impl IntoIterator<Item = &'a str>,
    ) -> Result<()> {
        wait_for_resource::all_namespaces(
            &self.interface.kubectl_command,
            resources,
            wait_for,
            extra,
        )
        .await
    }

    /// Wait for a rollout of a `resource` to complete.
    /// Use `extra` to pass additional arguments to `kubectl`.
    pub async fn wait_for_rollout<'a>(
        &self,
        namespace: &str,
        resource: &str,
        extra: impl IntoIterator<Item = &'a str>,
    ) -> Result<()> {
        wait_for_rollout::run(&self.interface.kubectl_command, namespace, resource, extra).await
    }
}
