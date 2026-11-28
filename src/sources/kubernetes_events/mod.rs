//! Kubernetes events source.
//!
//! This source watches the Kubernetes Events API and emits each event as a Vector log event. It is
//! designed for singleton deployments that run once per cluster.
//!
//! With leader election enabled, replicas coordinate through a `coordination.k8s.io/v1` Lease.
//! The active leader additionally persists the last safely handled Kubernetes `resourceVersion`
//! for each watch stream as an annotation on that Lease, piggybacked on the periodic renewal
//! write. When leadership changes hands, the new leader resumes each watch from its API-server
//! checkpoint. Checkpoints advance only after events are handed to the topology, so failover
//! preserves at-least-once delivery into Vector; duplicates remain possible in the failover window
//! and downstream consumers can deduplicate on `event_uid` plus the event's `resourceVersion`.

mod config;
mod deduper;
mod leader_election;
mod source;
mod watcher;

use chrono::{DateTime, Utc};
use k8s_openapi::jiff::Timestamp as KubeTimestamp;

fn kube_timestamp_to_chrono(timestamp: KubeTimestamp) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_micros(timestamp.as_microsecond())
}

#[cfg(test)]
mod test_util {
    use super::KubeTimestamp;
    use chrono::{DateTime, Utc};
    use k8s_openapi::api::events::v1::Event as KubeEvent;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{MicroTime, ObjectMeta};

    pub(super) fn kube_timestamp(timestamp: DateTime<Utc>) -> KubeTimestamp {
        KubeTimestamp::from_microsecond(timestamp.timestamp_micros())
            .expect("timestamp should fit in Kubernetes timestamp range")
    }

    pub(super) fn make_event(
        uid: &str,
        resource_version: &str,
        timestamp: DateTime<Utc>,
    ) -> KubeEvent {
        KubeEvent {
            metadata: ObjectMeta {
                uid: Some(uid.to_string()),
                resource_version: Some(resource_version.to_string()),
                ..ObjectMeta::default()
            },
            event_time: Some(MicroTime(kube_timestamp(timestamp))),
            note: Some("test".to_string()),
            ..KubeEvent::default()
        }
    }
}
