//! The test framework main entry point.

use super::{
    log_lookup, namespace, test_pod, vector, wait_for_resource, wait_for_rollout, Interface, Result,
};

pub struct Framework {
    interface: Interface,
}

impl Framework {
    /// Create a new [`Framework`].
    pub fn new(interface: Interface) -> Self {
        Self { interface }
    }

    pub fn vector(&self, namespace: &str, custom_resource: &str) -> Result<vector::Manager> {
        let manager = vector::Manager::new(
            self.interface.deploy_vector_command.as_str(),
            namespace,
            custom_resource,
        )?;
        manager.up()?;
        Ok(manager)
    }

    pub fn namespace(&self, namespace: &str) -> Result<namespace::Manager> {
        let manager = namespace::Manager::new(&self.interface.kubectl_command, namespace)?;
        manager.up()?;
        Ok(manager)
    }

    pub fn test_pod(&self, config: test_pod::Config) -> Result<test_pod::Manager> {
        let manager = test_pod::Manager::new(&self.interface.kubectl_command, config)?;
        manager.up()?;
        Ok(manager)
    }

    pub fn logs(&self, namespace: &str, resource: &str) -> Result<log_lookup::Reader> {
        log_lookup::logs(&self.interface.kubectl_command, namespace, resource)
    }

    pub fn wait<'a>(
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
    }

    pub fn wait_all_namespaces<'a>(
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
    }

    pub fn wait_for_rollout<'a>(
        &self,
        namespace: &str,
        resource: &str,
        extra: impl IntoIterator<Item = &'a str>,
    ) -> Result<()> {
        wait_for_rollout::run(&self.interface.kubectl_command, namespace, resource, extra)
    }
}
