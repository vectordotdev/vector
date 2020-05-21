//! Annotates events with pod metadata.

#![deny(missing_docs)]

use super::{path_helpers::parse_log_file_path, FILE_KEY};
use crate::{
    event::{LogEvent, Value},
    kubernetes as k8s, Event,
};
use evmap10::ReadHandle;
use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::ObjectMeta};
use string_cache::Atom;

/// Annotate the event with pod metadata.
pub struct PodMetadataAnnotator {
    pods_state_reader: ReadHandle<String, k8s::reflector::Value<Pod>>,
}

impl PodMetadataAnnotator {
    /// Create a new [`PodMetadataAnnotator`].
    pub fn new(pods_state_reader: ReadHandle<String, k8s::reflector::Value<Pod>>) -> Self {
        Self { pods_state_reader }
    }
}

impl PodMetadataAnnotator {
    /// Annotates an event with the information from the [`Pod::metadata`].
    /// The event has to be obtained from kubernetes log file, and have a
    /// [`FILE_KEY`] field set with a file that the line came from.
    pub fn annotate(&self, event: &mut Event) -> Option<()> {
        let log = event.as_mut_log();
        let uid = pod_uid_from_log_event(log)?;
        let guard = self.pods_state_reader.get(&uid)?;
        let entry = guard.get_one()?;
        let pod: &Pod = entry.as_ref();
        let metadata = pod.metadata.as_ref()?;
        annotate_from_metadata(log, &metadata);
        Some(())
    }
}

fn pod_uid_from_log_event(log: &LogEvent) -> Option<String> {
    let value = log.get(&Atom::from(FILE_KEY))?;
    let file = match value {
        Value::Bytes(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        _ => return None,
    };
    let info = parse_log_file_path(file.as_str())?;
    Some(info.pod_uid.to_owned())
}

fn annotate_from_metadata(log: &mut LogEvent, metadata: &ObjectMeta) {
    for (ref key, ref val) in [
        ("kubernetes.pod_name", &metadata.name),
        ("kubernetes.pod_namespace", &metadata.namespace),
        ("kubernetes.pod_uid", &metadata.uid),
    ]
    .iter()
    {
        if let Some(val) = val {
            log.insert(key, val);
        }
    }

    if let Some(labels) = &metadata.labels {
        for (key, val) in labels.iter() {
            log.insert(format!("{}.{}", "kubernetes.pod_labels", key), val);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pod_uid_from_log_event() {
        let cases = vec![
            // Valid inputs.
            (
                {
                    let mut log = LogEvent::default();
                    log.insert(
                        FILE_KEY,
                        "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/sandbox0-container0-name/1.log",
                    );
                    log
                },
                Some("sandbox0-uid"),
            ),
            // Invalid inputs.
            (LogEvent::default(), None),
            (
                {
                    let mut log = LogEvent::default();
                    log.insert(FILE_KEY, "qwerty");
                    log
                },
                None,
            ),
        ];

        for (log, expected) in cases.into_iter() {
            assert_eq!(
                pod_uid_from_log_event(&log),
                expected.map(ToOwned::to_owned)
            );
        }
    }
}
