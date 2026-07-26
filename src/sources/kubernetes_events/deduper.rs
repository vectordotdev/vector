//! Deduplication state for Kubernetes events, keyed by object UID and `resourceVersion`.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use k8s_openapi::api::events::v1::Event as KubeEvent;

pub(super) struct Deduper {
    entries: HashMap<String, CachedEvent>,
    retention: Duration,
}

struct CachedEvent {
    event: Option<KubeEvent>,
    resource_version: String,
    last_seen: Instant,
}

#[derive(Debug)]
pub(super) enum DedupResult {
    Added,
    Updated { previous: Option<Box<KubeEvent>> },
    Duplicate,
}

/// A dedupe entry that is only committed once the corresponding Vector event has been handed to
/// the topology, so that undelivered events are re-evaluated instead of silently marked seen.
pub(super) struct PendingDedupeRecord {
    pub(super) uid: String,
    pub(super) resource_version: String,
    pub(super) event: KubeEvent,
}

impl Deduper {
    pub(super) fn new(retention: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            retention,
        }
    }

    pub(super) fn evaluate(
        &mut self,
        uid: &str,
        resource_version: &str,
        include_previous: bool,
    ) -> DedupResult {
        match self.entries.get_mut(uid) {
            Some(entry) => {
                if resource_version == entry.resource_version {
                    entry.last_seen = Instant::now();
                    DedupResult::Duplicate
                } else {
                    let previous = include_previous
                        .then(|| entry.event.clone())
                        .flatten()
                        .map(Box::new);
                    DedupResult::Updated { previous }
                }
            }
            None => DedupResult::Added,
        }
    }

    #[cfg(test)]
    fn contains(&mut self, uid: &str) -> bool {
        self.prune();
        self.entries.contains_key(uid)
    }

    pub(super) fn commit(&mut self, record: PendingDedupeRecord) {
        self.entries.insert(
            record.uid,
            CachedEvent {
                event: Some(record.event),
                resource_version: record.resource_version,
                last_seen: Instant::now(),
            },
        );
    }

    #[cfg(test)]
    fn record(
        &mut self,
        uid: String,
        resource_version: String,
        event: &KubeEvent,
        _timestamp: chrono::DateTime<chrono::Utc>,
        include_previous: bool,
    ) -> DedupResult {
        let result = self.evaluate(&uid, &resource_version, include_previous);
        if !matches!(result, DedupResult::Duplicate) {
            self.commit(PendingDedupeRecord {
                uid,
                resource_version,
                event: event.clone(),
            });
        }
        result
    }

    pub(super) fn prune(&mut self) {
        if self.retention.is_zero() {
            return;
        }
        let retention = self.retention;
        self.entries
            .retain(|_, entry| entry.last_seen.elapsed() <= retention);
    }

    pub(super) fn remove(&mut self, uid: &str) {
        self.entries.remove(uid);
    }

    /// Retains resource versions for replay suppression while discarding payloads that may no
    /// longer represent the version most recently delivered by another leader.
    pub(super) fn invalidate_previous_events(&mut self) {
        for entry in self.entries.values_mut() {
            entry.event = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::make_event;
    use super::*;
    use chrono::{Duration as ChronoDuration, TimeZone, Utc};

    #[test]
    fn deduper_adds_and_updates_events() {
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let first_ts = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let later_ts = first_ts + ChronoDuration::seconds(10);

        let event_added = make_event("uid", "1", first_ts);
        assert!(matches!(
            deduper.record(
                "uid".to_string(),
                "1".to_string(),
                &event_added,
                first_ts,
                false
            ),
            DedupResult::Added
        ));

        // Duplicate resourceVersion yields no update.
        assert!(matches!(
            deduper.record(
                "uid".to_string(),
                "1".to_string(),
                &event_added,
                first_ts,
                true
            ),
            DedupResult::Duplicate
        ));

        let updated_event = make_event("uid", "2", later_ts);
        match deduper.record(
            "uid".to_string(),
            "2".to_string(),
            &updated_event,
            later_ts,
            true,
        ) {
            DedupResult::Updated { previous } => {
                let previous = previous.expect("previous event expected");
                assert_eq!(
                    previous.metadata.resource_version.as_deref(),
                    Some("1"),
                    "previous event should reflect the prior resourceVersion"
                );
            }
            other => panic!("expected DedupResult::Updated, got {other:?}"),
        }
    }

    #[test]
    fn deduper_treats_resource_versions_as_opaque_values() {
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let first_ts = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let later_ts = first_ts + ChronoDuration::seconds(10);
        let event_added = make_event("uid", "z", first_ts);

        assert!(matches!(
            deduper.record(
                "uid".to_string(),
                "z".to_string(),
                &event_added,
                first_ts,
                false
            ),
            DedupResult::Added
        ));

        let updated_event = make_event("uid", "a", later_ts);
        match deduper.record(
            "uid".to_string(),
            "a".to_string(),
            &updated_event,
            later_ts,
            true,
        ) {
            DedupResult::Updated { previous } => {
                let previous = previous.expect("previous event expected");
                assert_eq!(previous.metadata.resource_version.as_deref(), Some("z"));
            }
            other => panic!("expected DedupResult::Updated, got {other:?}"),
        }
    }

    #[test]
    fn deduper_defers_new_resource_version_until_commit() {
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let first_ts = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let later_ts = first_ts + ChronoDuration::seconds(10);
        let first_event = make_event("uid", "1", first_ts);
        let updated_event = make_event("uid", "2", later_ts);

        assert!(matches!(
            deduper.evaluate("uid", "1", false),
            DedupResult::Added
        ));
        assert!(
            !deduper.entries.contains_key("uid"),
            "new events should not be marked seen before delivery"
        );

        deduper.commit(PendingDedupeRecord {
            uid: "uid".to_string(),
            resource_version: "1".to_string(),
            event: first_event,
        });

        assert!(matches!(
            deduper.evaluate("uid", "2", true),
            DedupResult::Updated { .. }
        ));
        assert_eq!(
            deduper.entries.get("uid").and_then(|entry| entry
                .event
                .as_ref()
                .and_then(|event| event.metadata.resource_version.as_deref())),
            Some("1"),
            "updates should not replace the cached event before delivery"
        );

        deduper.commit(PendingDedupeRecord {
            uid: "uid".to_string(),
            resource_version: "2".to_string(),
            event: updated_event,
        });
        assert_eq!(
            deduper.entries.get("uid").and_then(|entry| entry
                .event
                .as_ref()
                .and_then(|event| event.metadata.resource_version.as_deref())),
            Some("2")
        );
    }

    #[test]
    fn deduper_prunes_expired_entries() {
        let retention = Duration::from_millis(5);
        let mut deduper = Deduper::new(retention);
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let event = make_event("uid", "1", timestamp);

        assert!(matches!(
            deduper.record("uid".to_string(), "1".to_string(), &event, timestamp, false),
            DedupResult::Added
        ));

        // Age the cached entry beyond the retention window.
        if let Some(entry) = deduper.entries.get_mut("uid") {
            entry.last_seen = Instant::now() - retention - Duration::from_millis(1);
        }

        deduper.prune();
        assert!(
            !deduper.entries.contains_key("uid"),
            "entry should be pruned after retention elapses"
        );
    }

    #[test]
    fn deduper_contains_prunes_expired_entries() {
        let retention = Duration::from_millis(5);
        let mut deduper = Deduper::new(retention);
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let event = make_event("uid", "1", timestamp);

        deduper.commit(PendingDedupeRecord {
            uid: "uid".to_string(),
            resource_version: "1".to_string(),
            event,
        });

        if let Some(entry) = deduper.entries.get_mut("uid") {
            entry.last_seen = Instant::now() - retention - Duration::from_millis(1);
        }

        assert!(
            !deduper.contains("uid"),
            "contains should ignore entries past the retention window"
        );
        assert!(!deduper.entries.contains_key("uid"));
    }

    #[test]
    fn deduper_refreshes_ttl_for_replayed_resource_version() {
        let retention = Duration::from_secs(60);
        let mut deduper = Deduper::new(retention);
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let event = make_event("uid", "1", timestamp);

        assert!(matches!(
            deduper.record("uid".to_string(), "1".to_string(), &event, timestamp, false),
            DedupResult::Added
        ));

        if let Some(entry) = deduper.entries.get_mut("uid") {
            entry.last_seen = Instant::now() - retention - Duration::from_secs(1);
        }

        assert!(matches!(
            deduper.record("uid".to_string(), "1".to_string(), &event, timestamp, false),
            DedupResult::Duplicate
        ));

        deduper.prune();
        assert!(
            deduper.entries.contains_key("uid"),
            "same resourceVersion replay should refresh the dedupe retention"
        );
    }

    #[test]
    fn invalidating_previous_events_preserves_resource_version_dedupe() {
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let first = make_event("uid", "1", timestamp);

        deduper.commit(PendingDedupeRecord {
            uid: "uid".to_string(),
            resource_version: "1".to_string(),
            event: first.clone(),
        });
        deduper.invalidate_previous_events();

        assert!(matches!(
            deduper.evaluate("uid", "1", true),
            DedupResult::Duplicate
        ));
        assert!(matches!(
            deduper.evaluate("uid", "2", true),
            DedupResult::Updated { previous: None }
        ));

        deduper.commit(PendingDedupeRecord {
            uid: "uid".to_string(),
            resource_version: "2".to_string(),
            event: make_event("uid", "2", timestamp),
        });
        assert!(matches!(
            deduper.evaluate("uid", "3", true),
            DedupResult::Updated { previous: Some(_) }
        ));
    }
}
