//! Kubernetes events source.
//!
//! This source watches the Kubernetes Events API and emits each event as a Vector log event. It is
//! designed for singleton deployments that run once per cluster.
//!
//! With leader election enabled, replicas coordinate through a `coordination.k8s.io/v1` Lease.
//! The active leader additionally persists a delivery watermark (the newest event timestamp it
//! has successfully forwarded) as an annotation on that Lease, piggybacked on the periodic
//! renewal write. When leadership changes hands, the new leader reads the watermark and skips
//! events at or below it (minus a configurable grace window for out-of-order timestamps) during
//! the initial watch replay, instead of re-emitting everything within `max_event_age_seconds`.
//! The watermark only ever advances over events that were actually handed to the topology, so
//! failover preserves at-least-once delivery; duplicates remain possible in the failover window
//! and downstream consumers can deduplicate on `event_uid` plus the event's `resourceVersion`.

mod config;
mod deduper;
mod leader_election;
mod source;

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
