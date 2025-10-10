use k8s_openapi::api::core::v1::Pod;
use serde::{Deserialize, Serialize};

/// Pod information struct that contains essential details for log fetching
#[derive(Clone, Debug, Serialize, Deserialize)]
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

/// Error type for failed Pod to PodInfo conversion
#[derive(Debug, Clone)]
pub enum PodConversionError {
    MissingName,
    MissingUid,
}

impl std::fmt::Display for PodConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PodConversionError::MissingName => write!(f, "Pod is missing required name field"),
            PodConversionError::MissingUid => write!(f, "Pod is missing required UID field"),
        }
    }
}

impl std::error::Error for PodConversionError {}

impl TryFrom<&Pod> for PodInfo {
    type Error = PodConversionError;

    fn try_from(pod: &Pod) -> Result<Self, Self::Error> {
        let metadata = &pod.metadata;

        let name = metadata
            .name
            .as_ref()
            .ok_or(PodConversionError::MissingName)?
            .clone();

        let namespace = metadata.namespace.as_ref().cloned().unwrap_or_default();

        let uid = metadata
            .uid
            .as_ref()
            .ok_or(PodConversionError::MissingUid)?
            .clone();

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

        Ok(PodInfo {
            name,
            namespace,
            uid,
            phase,
            containers,
        })
    }
}

impl TryFrom<Pod> for PodInfo {
    type Error = PodConversionError;

    fn try_from(pod: Pod) -> Result<Self, Self::Error> {
        Self::try_from(&pod)
    }
}

impl PodInfo {
    /// Check if this pod is in Running phase
    pub fn is_running(&self) -> bool {
        self.phase
            .as_ref()
            .map_or(false, |phase| phase == "Running")
    }
}
