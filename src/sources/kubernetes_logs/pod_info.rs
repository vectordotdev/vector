use k8s_openapi::api::core::v1::Pod;
use serde::{Deserialize, Serialize};

/// Pod information struct that contains essential details for log fetching
#[derive(Clone, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub struct PodInfo {
    /// Pod name
    pub name: String,
    /// Pod namespace
    pub namespace: String,
    /// Pod UID for uniqueness
    pub uid: String,
    /// Pod phase (Running, Pending, etc.)
    pub phase: Option<String>,
    /// Container names within the pod
    pub containers: Vec<String>,
}

impl From<&Pod> for PodInfo {
    fn from(pod: &Pod) -> Self {
        let metadata = &pod.metadata;

        let name = metadata.name.as_ref().cloned().unwrap_or_default();

        let namespace = metadata.namespace.as_ref().cloned().unwrap_or_default();

        let uid = metadata.uid.as_ref().cloned().unwrap_or_default();

        let phase = pod.status.as_ref().and_then(|status| status.phase.clone());

        let containers = pod
            .spec
            .as_ref()
            .map(|spec| {
                spec.containers
                    .iter()
                    .map(|container| container.name.clone())
                    .collect()
            })
            .unwrap_or_default();

        PodInfo {
            name,
            namespace,
            uid,
            phase,
            containers,
        }
    }
}
