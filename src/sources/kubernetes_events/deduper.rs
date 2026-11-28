//! Deduplication state for Kubernetes events, keyed by object UID and `resourceVersion`.

use std::{
    num::NonZeroUsize,
    time::{Duration, Instant},
};

use k8s_openapi::api::events::v1::Event as KubeEvent;
use lru::LruCache;

/// Deduplication cache keyed by event UID.
///
/// Entries are kept in least-recently-seen order, so expired entries are always at the LRU end
/// and can be pruned without scanning the whole cache. The cache is bounded both by `retention`
/// and by `max_entries`.
pub(super) struct Deduper {
    entries: LruCache<String, CachedEvent>,
    retention: Duration,
    max_entries: NonZeroUsize,
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
    pub(super) event: Option<KubeEvent>,
}

impl Deduper {
    pub(super) fn new(retention: Duration, max_entries: NonZeroUsize) -> Self {
        Self {
            // Unbounded so that capacity is not preallocated; `max_entries` is enforced in
            // `commit`.
            entries: LruCache::unbounded(),
            retention,
            max_entries,
        }
    }

    pub(super) fn evaluate(
        &mut self,
        uid: &str,
        resource_version: &str,
        include_previous: bool,
    ) -> DedupResult {
        let now = Instant::now();
        self.prune_at(now);

        // `peek_mut` keeps the LRU order untouched; recency is only refreshed together with
        // `last_seen` so that the LRU end always holds the oldest entry.
        match self.entries.peek_mut(uid) {
            Some(entry) => {
                if resource_version == entry.resource_version {
                    entry.last_seen = now;
                    self.entries.promote(uid);
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
        self.entries.contains(uid)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn commit(&mut self, record: PendingDedupeRecord) {
        let now = Instant::now();
        self.prune_at(now);
        self.entries.put(
            record.uid,
            CachedEvent {
                event: record.event,
                resource_version: record.resource_version,
                last_seen: now,
            },
        );

        let mut evicted = 0usize;
        while self.entries.len() > self.max_entries.get() {
            self.entries.pop_lru();
            evicted += 1;
        }
        if evicted > 0 {
            debug!(
                message = "Kubernetes events deduplication cache is full; evicted least recently seen entries.",
                evicted,
                max_entries = self.max_entries.get(),
                internal_log_rate_limit = true,
            );
        }
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
                event: Some(event.clone()),
            });
        }
        result
    }

    pub(super) fn prune(&mut self) {
        self.prune_at(Instant::now());
    }

    /// Removes expired entries. Because entries are ordered by `last_seen`, this only touches
    /// expired entries plus one live entry.
    fn prune_at(&mut self, now: Instant) {
        while self
            .entries
            .peek_lru()
            .is_some_and(|(_, entry)| entry.is_expired(now, self.retention))
        {
            self.entries.pop_lru();
        }
    }

    pub(super) fn remove(&mut self, uid: &str) {
        self.entries.pop(uid);
    }

    /// Retains resource versions for replay suppression while discarding payloads that may no
    /// longer represent the version most recently delivered by another leader.
    pub(super) fn invalidate_previous_events(&mut self) {
        for (_, entry) in &mut self.entries {
            entry.event = None;
        }
    }
}

impl CachedEvent {
    fn is_expired(&self, now: Instant, retention: Duration) -> bool {
        now.saturating_duration_since(self.last_seen) > retention
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::make_event;
    use super::*;
    use chrono::{Duration as ChronoDuration, TimeZone, Utc};

    fn ago(duration: Duration) -> Instant {
        Instant::now()
            .checked_sub(duration)
            .expect("test durations fit before the current instant")
    }

    #[test]
    fn deduper_adds_and_updates_events() {
        let mut deduper = Deduper::new(Duration::from_secs(60), NonZeroUsize::MAX);
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
        let mut deduper = Deduper::new(Duration::from_secs(60), NonZeroUsize::MAX);
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
        let mut deduper = Deduper::new(Duration::from_secs(60), NonZeroUsize::MAX);
        let first_ts = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let later_ts = first_ts + ChronoDuration::seconds(10);
        let first_event = make_event("uid", "1", first_ts);
        let updated_event = make_event("uid", "2", later_ts);

        assert!(matches!(
            deduper.evaluate("uid", "1", false),
            DedupResult::Added
        ));
        assert!(
            !deduper.entries.contains("uid"),
            "new events should not be marked seen before delivery"
        );

        deduper.commit(PendingDedupeRecord {
            uid: "uid".to_string(),
            resource_version: "1".to_string(),
            event: Some(first_event),
        });

        assert!(matches!(
            deduper.evaluate("uid", "2", true),
            DedupResult::Updated { .. }
        ));
        assert_eq!(
            deduper.entries.peek("uid").and_then(|entry| entry
                .event
                .as_ref()
                .and_then(|event| event.metadata.resource_version.as_deref())),
            Some("1"),
            "updates should not replace the cached event before delivery"
        );

        deduper.commit(PendingDedupeRecord {
            uid: "uid".to_string(),
            resource_version: "2".to_string(),
            event: Some(updated_event),
        });
        assert_eq!(
            deduper.entries.peek("uid").and_then(|entry| entry
                .event
                .as_ref()
                .and_then(|event| event.metadata.resource_version.as_deref())),
            Some("2")
        );
    }

    #[test]
    fn deduper_prunes_expired_entries() {
        let retention = Duration::from_millis(5);
        let mut deduper = Deduper::new(retention, NonZeroUsize::MAX);
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let event = make_event("uid", "1", timestamp);

        assert!(matches!(
            deduper.record("uid".to_string(), "1".to_string(), &event, timestamp, false),
            DedupResult::Added
        ));

        // Age the cached entry beyond the retention window.
        if let Some(entry) = deduper.entries.peek_mut("uid") {
            entry.last_seen = ago(retention + Duration::from_millis(1));
        }

        deduper.prune();
        assert!(
            !deduper.entries.contains("uid"),
            "entry should be pruned after retention elapses"
        );
    }

    #[test]
    fn deduper_contains_prunes_expired_entries() {
        let retention = Duration::from_millis(5);
        let mut deduper = Deduper::new(retention, NonZeroUsize::MAX);
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let event = make_event("uid", "1", timestamp);

        deduper.commit(PendingDedupeRecord {
            uid: "uid".to_string(),
            resource_version: "1".to_string(),
            event: Some(event),
        });

        if let Some(entry) = deduper.entries.peek_mut("uid") {
            entry.last_seen = ago(retention + Duration::from_millis(1));
        }

        assert!(
            !deduper.contains("uid"),
            "contains should ignore entries past the retention window"
        );
        assert!(!deduper.entries.contains("uid"));
    }

    #[test]
    fn deduper_refreshes_ttl_for_replayed_resource_version() {
        let retention = Duration::from_secs(60);
        let mut deduper = Deduper::new(retention, NonZeroUsize::MAX);
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let event = make_event("uid", "1", timestamp);

        assert!(matches!(
            deduper.record("uid".to_string(), "1".to_string(), &event, timestamp, false),
            DedupResult::Added
        ));

        if let Some(entry) = deduper.entries.peek_mut("uid") {
            entry.last_seen = ago(retention.saturating_sub(Duration::from_secs(1)));
        }

        assert!(matches!(
            deduper.record("uid".to_string(), "1".to_string(), &event, timestamp, false),
            DedupResult::Duplicate
        ));

        deduper.prune();
        assert!(
            deduper.entries.contains("uid"),
            "same resourceVersion replay should refresh the dedupe retention"
        );
    }

    #[test]
    fn deduper_lazily_expires_accessed_uid() {
        let retention = Duration::from_secs(60);
        let mut deduper = Deduper::new(retention, NonZeroUsize::MAX);
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let event = make_event("uid", "1", timestamp);

        deduper.commit(PendingDedupeRecord {
            uid: "uid".to_string(),
            resource_version: "1".to_string(),
            event: Some(event),
        });
        if let Some(entry) = deduper.entries.peek_mut("uid") {
            entry.last_seen = ago(retention + Duration::from_secs(1));
        }

        assert!(matches!(
            deduper.evaluate("uid", "1", false),
            DedupResult::Added
        ));
        assert!(!deduper.entries.contains("uid"));
    }

    #[test]
    fn deduper_evicts_least_recently_seen_entries_when_full() {
        let mut deduper = Deduper::new(Duration::from_secs(3600), NonZeroUsize::new(2).unwrap());
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();

        for uid in ["a", "b"] {
            deduper.record(
                uid.to_string(),
                "1".to_string(),
                &make_event(uid, "1", timestamp),
                timestamp,
                false,
            );
        }
        // Seeing `a` again makes `b` the least recently seen entry.
        assert!(matches!(
            deduper.evaluate("a", "1", false),
            DedupResult::Duplicate
        ));

        deduper.record(
            "c".to_string(),
            "1".to_string(),
            &make_event("c", "1", timestamp),
            timestamp,
            false,
        );

        assert_eq!(deduper.len(), 2);
        assert!(deduper.contains("a"));
        assert!(
            !deduper.contains("b"),
            "least recently seen entry should be evicted"
        );
        assert!(deduper.contains("c"));
    }

    #[test]
    fn deduper_prunes_expired_entries_behind_updated_entries() {
        let retention = Duration::from_secs(60);
        let mut deduper = Deduper::new(retention, NonZeroUsize::MAX);
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();

        for uid in ["old", "live"] {
            deduper.commit(PendingDedupeRecord {
                uid: uid.to_string(),
                resource_version: "1".to_string(),
                event: Some(make_event(uid, "1", timestamp)),
            });
        }
        // Evaluating an update must not move `old` ahead of `live` without refreshing it.
        assert!(matches!(
            deduper.evaluate("old", "2", false),
            DedupResult::Updated { .. }
        ));
        if let Some(entry) = deduper.entries.peek_mut("old") {
            entry.last_seen = ago(retention + Duration::from_secs(1));
        }

        deduper.prune();
        assert!(!deduper.entries.contains("old"));
        assert!(deduper.entries.contains("live"));
    }

    #[test]
    fn invalidating_previous_events_preserves_resource_version_dedupe() {
        let mut deduper = Deduper::new(Duration::from_secs(60), NonZeroUsize::MAX);
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let first = make_event("uid", "1", timestamp);

        deduper.commit(PendingDedupeRecord {
            uid: "uid".to_string(),
            resource_version: "1".to_string(),
            event: Some(first),
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
            event: Some(make_event("uid", "2", timestamp)),
        });
        assert!(matches!(
            deduper.evaluate("uid", "3", true),
            DedupResult::Updated { previous: Some(_) }
        ));
    }
}
