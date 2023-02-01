//! The test framework main entry point.

use super::{
    exec_tail, kubernetes_version, log_lookup, namespace, pod, port_forward, restart_rollout,
    test_pod, up_down, vector, wait_for_resource, wait_for_rollout, Interface, PortForwarder,
    Reader, Result,
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

    /// Deploy a Helm chart into a cluster.
    pub async fn helm_chart(
        &self,
        namespace: &str,
        helm_chart: &str,
        release_name: &str,
        helm_repo: &str,
        config: vector::Config<'_>,
    ) -> Result<up_down::Manager<vector::CommandBuilder>> {
        let env = vec![("CHART_REPO".to_owned(), helm_repo.to_owned())];
        let mut manager = vector::manager(
            self.interface.deploy_chart_command.as_str(),
            namespace,
            helm_chart,
            release_name,
            config,
            Some(env),
        )?;
        manager.up().await?;
        Ok(manager)
    }

    /// Create a new namespace.
    pub async fn namespace(
        &self,
        config: namespace::Config,
    ) -> Result<up_down::Manager<namespace::CommandBuilder>> {
        let mut manager = namespace::manager(&self.interface.kubectl_command, config);
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

    /// Initialize port forward for a particular `resource` in a particular
    /// `namespace` with a particular pair of local/resource ports.
    pub fn port_forward(
        &self,
        namespace: &str,
        resource: &str,
        local_port: u16,
        resource_port: u16,
    ) -> Result<PortForwarder> {
        port_forward(
            &self.interface.kubectl_command,
            namespace,
            resource,
            local_port,
            resource_port,
        )
    }

    /// Exect a `kubectl --version`command returning a K8sVersion Struct
    /// containing all version information  of the running Kubernetes test cluster.
    pub async fn kubernetes_version(&self) -> Result<kubernetes_version::K8sVersion> {
        kubernetes_version::get(&self.interface.kubectl_command).await
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

    /// Trigger a restart for a rollout of a `resource`.
    /// Use `extr
    pub async fn restart_rollout<'a>(
        &self,
        namespace: &str,
        resources: &str,
        extra: impl IntoIterator<Item = &'a str>,
    ) -> Result<()> {
        restart_rollout::run(&self.interface.kubectl_command, namespace, resources, extra).await
    }

    /// Gets the node for a given pod.
    async fn get_node_for_pod(&self, namespace: &str, pod: &str) -> Result<String> {
        pod::get_node(&self.interface.kubectl_command, namespace, pod).await
    }

    /// Gets the name of the pod implementing the service on the given node.
    async fn get_pod_on_node(&self, namespace: &str, node: &str, service: &str) -> Result<String> {
        pod::get_pod_on_node(&self.interface.kubectl_command, namespace, node, service).await
    }

    /// Sets a label on all nodes.
    pub async fn label_nodes(&self, label: &str) -> Result<String> {
        pod::label_nodes(&self.interface.kubectl_command, label).await
    }

    /// Return the Vector pod that is deployed on the same node as the given pod. We want to make
    /// sure we are scanning the Vector instance that is deployed with the test pod.
    pub async fn get_vector_pod_with_pod(
        &self,
        pod_namespace: &str,
        pod_name: &str,
        vector_pod_namespace: &str,
        vector_pod_name: &str,
    ) -> Result<String> {
        let node = self
            .get_node_for_pod(pod_namespace, pod_name)
            .await
            .map_err(|_| "need the node name")?;

        Ok(self
            .get_pod_on_node(vector_pod_namespace, &node, vector_pod_name)
            .await
            .map_err(|_| "cant get the vector pod running on the test node")?)
    }
}
