#![deny(missing_docs)]

//! Kubernetes events source.
//!
//! This source watches the Kubernetes Events API and emits each event as a Vector log event. It is
//! designed for singleton deployments that run once per cluster.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::PathBuf,
    pin::Pin,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use futures::{Stream, StreamExt, stream::SelectAll};
use http_1::{HeaderName, HeaderValue};
use k8s_openapi::api::events::v1::Event as KubeEvent;
use kube::{
    Api, Client, Config as ClientConfig,
    config::{self, KubeConfigOptions},
    runtime::watcher,
};
use tokio::select;
use vector_lib::{
    config::{LegacyKey, LogNamespace, log_schema},
    configurable::configurable_component,
    internal_event::{ComponentEventsDropped, INTENTIONAL},
    lookup::{event_path, owned_value_path},
    schema::Definition,
};
use vrl::value::{Kind, kind::Collection};

use crate::{
    SourceSender,
    config::{DataType, SourceConfig, SourceContext, SourceOutput},
    event::{EstimatedJsonEncodedSizeOf, Event, LogEvent},
    internal_events::{
        KubernetesEventsReceived, KubernetesEventsSerializationError, KubernetesEventsWatchError,
        StreamClosedError,
    },
    shutdown::ShutdownSignal,
};

const DEFAULT_MAX_EVENT_AGE_SECS: u64 = 3600;
const DEFAULT_DEDUPE_RETENTION_SECS: u64 = 900;
const DEFAULT_WATCH_TIMEOUT_SECS: u32 = 290;

type WatchItem = (Option<String>, watcher::Result<watcher::Event<KubeEvent>>);
type WatchStream = Pin<Box<dyn Stream<Item = WatchItem> + Send>>;

/// Configuration for the `kubernetes_events` source.
#[configurable_component(source(
    "kubernetes_events",
    "Collect cluster events from the Kubernetes API."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct KubernetesEventsConfig {
    /// Path to a kubeconfig file. If omitted, in-cluster configuration or the local kubeconfig is used.
    #[configurable(metadata(docs::examples = "/path/to/kubeconfig"))]
    kube_config_file: Option<PathBuf>,

    /// Limits the collection to the specified namespaces. If empty, all namespaces are watched.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "kube-system"))]
    namespaces: Vec<String>,

    /// Field selector applied to the events list/watch request.
    #[configurable(metadata(docs::examples = "regarding.kind=Pod"))]
    field_selector: Option<String>,

    /// Label selector applied to the events list/watch request.
    #[configurable(metadata(docs::examples = "type=Warning"))]
    label_selector: Option<String>,

    /// Restricts the source to the specified event types (for example, `Warning`). Empty means all types.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "Warning"))]
    include_types: Vec<String>,

    /// Restricts the source to the specified reasons. Empty means all reasons.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "FailedScheduling"))]
    include_reasons: Vec<String>,

    /// Restricts the source to the specified involved object kinds. Empty means all kinds.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "Pod"))]
    include_involved_object_kinds: Vec<String>,

    /// Maximum age of an event to forward.
    #[serde(default = "default_max_event_age_seconds")]
    #[configurable(metadata(docs::type_unit = "seconds", docs::human_name = "Maximum Event Age"))]
    max_event_age_seconds: u64,

    /// Retention window for deduplication state.
    #[serde(default = "default_dedupe_retention_seconds")]
    #[configurable(metadata(
        docs::type_unit = "seconds",
        docs::human_name = "Deduplication Retention"
    ))]
    dedupe_retention_seconds: u64,

    /// Timeout applied to the Kubernetes watch call.
    #[serde(default = "default_watch_timeout_seconds")]
    #[configurable(metadata(docs::type_unit = "seconds", docs::human_name = "Watch Timeout"))]
    watch_timeout_seconds: u32,

    /// When enabled, the previous version of the event is included in the emitted payload on updates.
    #[serde(default)]
    include_previous_event: bool,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,
}

impl Default for KubernetesEventsConfig {
    fn default() -> Self {
        Self {
            kube_config_file: None,
            namespaces: Vec::new(),
            field_selector: None,
            label_selector: None,
            include_types: Vec::new(),
            include_reasons: Vec::new(),
            include_involved_object_kinds: Vec::new(),
            max_event_age_seconds: DEFAULT_MAX_EVENT_AGE_SECS,
            dedupe_retention_seconds: DEFAULT_DEDUPE_RETENTION_SECS,
            watch_timeout_seconds: DEFAULT_WATCH_TIMEOUT_SECS,
            include_previous_event: false,
            log_namespace: None,
        }
    }
}

impl_generate_config_from_default!(KubernetesEventsConfig);

const fn default_max_event_age_seconds() -> u64 {
    DEFAULT_MAX_EVENT_AGE_SECS
}

const fn default_dedupe_retention_seconds() -> u64 {
    DEFAULT_DEDUPE_RETENTION_SECS
}

const fn default_watch_timeout_seconds() -> u32 {
    DEFAULT_WATCH_TIMEOUT_SECS
}

#[async_trait::async_trait]
#[typetag::serde(name = "kubernetes_events")]
impl SourceConfig for KubernetesEventsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let mut client_config = match &self.kube_config_file {
            Some(path) => {
                ClientConfig::from_custom_kubeconfig(
                    config::Kubeconfig::read_from(path)?,
                    &KubeConfigOptions::default(),
                )
                .await?
            }
            None => ClientConfig::infer().await?,
        };

        if let Ok(user_agent) = HeaderValue::from_str(&format!(
            "{}/{}",
            crate::built_info::PKG_NAME,
            crate::built_info::PKG_VERSION
        )) {
            client_config
                .headers
                .push((HeaderName::from_static("user-agent"), user_agent));
        }

        let client = Client::try_from(client_config)?;

        let source = KubernetesEventsSource::new(client, self.clone());

        Ok(Box::pin(source.run(cx.out, cx.shutdown, log_namespace)))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            schema_definition(log_namespace),
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

fn schema_definition(log_namespace: LogNamespace) -> Definition {
    let mut namespaces = BTreeSet::new();
    namespaces.insert(log_namespace);

    Definition::new_with_default_metadata(Kind::object(Collection::any()), namespaces)
        .with_standard_vector_source_metadata()
        .with_source_metadata(
            KubernetesEventsConfig::NAME,
            Some(LegacyKey::InsertIfEmpty(owned_value_path!("namespace"))),
            &owned_value_path!("namespace"),
            Kind::bytes().or_undefined(),
            Some("namespace"),
        )
        .with_source_metadata(
            KubernetesEventsConfig::NAME,
            Some(LegacyKey::InsertIfEmpty(owned_value_path!("verb"))),
            &owned_value_path!("verb"),
            Kind::bytes(),
            Some("verb"),
        )
        .with_source_metadata(
            KubernetesEventsConfig::NAME,
            Some(LegacyKey::InsertIfEmpty(owned_value_path!("event_uid"))),
            &owned_value_path!("event_uid"),
            Kind::bytes(),
            Some("event_uid"),
        )
        .with_source_metadata(
            KubernetesEventsConfig::NAME,
            Some(LegacyKey::InsertIfEmpty(owned_value_path!("reason"))),
            &owned_value_path!("reason"),
            Kind::bytes().or_undefined(),
            Some("reason"),
        )
        .with_source_metadata(
            KubernetesEventsConfig::NAME,
            Some(LegacyKey::InsertIfEmpty(owned_value_path!("type"))),
            &owned_value_path!("type"),
            Kind::bytes().or_undefined(),
            Some("event_type"),
        )
}

struct KubernetesEventsSource {
    client: Client,
    namespaces: Vec<String>,
    type_filter: Option<HashSet<String>>,
    reason_filter: Option<HashSet<String>>,
    kind_filter: Option<HashSet<String>>,
    max_event_age: Duration,
    dedupe_retention: Duration,
    watcher_config: watcher::Config,
    include_previous_event: bool,
}

impl KubernetesEventsSource {
    fn new(client: Client, config: KubernetesEventsConfig) -> Self {
        let type_filter = (!config.include_types.is_empty())
            .then(|| config.include_types.iter().map(|s| s.to_owned()).collect());
        let reason_filter = (!config.include_reasons.is_empty()).then(|| {
            config
                .include_reasons
                .iter()
                .map(|s| s.to_owned())
                .collect()
        });
        let kind_filter = (!config.include_involved_object_kinds.is_empty()).then(|| {
            config
                .include_involved_object_kinds
                .iter()
                .map(|s| s.to_owned())
                .collect()
        });

        let mut watcher_config = watcher::Config::default().timeout(config.watch_timeout_seconds);
        if let Some(selector) = &config.field_selector {
            watcher_config = watcher_config.fields(selector);
        }
        if let Some(selector) = &config.label_selector {
            watcher_config = watcher_config.labels(selector);
        }

        Self {
            client,
            namespaces: config.namespaces.clone(),
            type_filter,
            reason_filter,
            kind_filter,
            max_event_age: Duration::from_secs(config.max_event_age_seconds),
            dedupe_retention: Duration::from_secs(config.dedupe_retention_seconds),
            watcher_config,
            include_previous_event: config.include_previous_event,
        }
    }

    fn build_streams(&self) -> SelectAll<WatchStream> {
        let mut streams = SelectAll::new();

        if self.namespaces.is_empty() {
            let api: Api<KubeEvent> = Api::all(self.client.clone());
            streams.push(self.make_stream(api, None));
        } else {
            for namespace in &self.namespaces {
                let api: Api<KubeEvent> = Api::namespaced(self.client.clone(), namespace);
                streams.push(self.make_stream(api, Some(namespace.clone())));
            }
        }

        streams
    }

    fn make_stream(&self, api: Api<KubeEvent>, namespace: Option<String>) -> WatchStream {
        let cfg = self.watcher_config.clone();
        Box::pin(watcher(api, cfg).map(move |event| (namespace.clone(), event)))
    }

    async fn run(
        mut self,
        mut out: SourceSender,
        mut shutdown: ShutdownSignal,
        log_namespace: LogNamespace,
    ) -> Result<(), ()> {
        let mut streams = self.build_streams();
        let mut deduper = Deduper::new(self.dedupe_retention);

        loop {
            select! {
                _ = &mut shutdown => break,
                maybe_event = streams.next() => {
                    match maybe_event {
                        Some((namespace, Ok(event))) => {
                            if let Err(()) = self.handle_event(namespace.as_deref(), event, &mut out, log_namespace, &mut deduper).await {
                                return Err(());
                            }
                        }
                        Some((_, Err(error))) => {
                            emit!(KubernetesEventsWatchError { error });
                        }
                        None => break,
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_event(
        &mut self,
        namespace: Option<&str>,
        event: watcher::Event<KubeEvent>,
        out: &mut SourceSender,
        log_namespace: LogNamespace,
        deduper: &mut Deduper,
    ) -> Result<(), ()> {
        match event {
            watcher::Event::Apply(ev) | watcher::Event::InitApply(ev) => {
                self.process_apply_event(namespace, ev, out, log_namespace, deduper)
                    .await
            }
            watcher::Event::Delete(ev) => {
                if let Some(uid) = ev.metadata.uid.as_deref() {
                    deduper.remove(uid);
                }
                Ok(())
            }
            watcher::Event::Init => Ok(()),
            watcher::Event::InitDone => {
                deduper.prune();
                Ok(())
            }
        }
    }

    async fn process_apply_event(
        &mut self,
        namespace: Option<&str>,
        event: KubeEvent,
        out: &mut SourceSender,
        log_namespace: LogNamespace,
        deduper: &mut Deduper,
    ) -> Result<(), ()> {
        let uid = match event.metadata.uid.clone() {
            Some(uid) => uid,
            None => {
                emit!(ComponentEventsDropped::<INTENTIONAL> {
                    count: 1,
                    reason: "missing_uid"
                });
                return Ok(());
            }
        };

        let resource_version = match event.metadata.resource_version.clone() {
            Some(rv) => rv,
            None => {
                emit!(ComponentEventsDropped::<INTENTIONAL> {
                    count: 1,
                    reason: "missing_resource_version"
                });
                return Ok(());
            }
        };

        if !self.type_allowed(&event) || !self.reason_allowed(&event) || !self.kind_allowed(&event)
        {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "filtered"
            });
            return Ok(());
        }

        let timestamp = event_timestamp(&event);
        if self.is_older_than(timestamp) {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "expired"
            });
            return Ok(());
        }

        deduper.prune();

        let dedup_result = deduper.record(
            uid.clone(),
            resource_version.clone(),
            &event,
            timestamp,
            self.include_previous_event,
        );

        let (verb, previous) = match dedup_result {
            DedupResult::Duplicate => {
                emit!(ComponentEventsDropped::<INTENTIONAL> {
                    count: 1,
                    reason: "duplicate"
                });
                return Ok(());
            }
            DedupResult::Added => ("ADDED", None),
            DedupResult::Updated { previous } => ("UPDATED", previous),
        };

        let mut log = LogEvent::default();
        if let Some(message_path) = log_schema().message_key_target_path()
            && let Some(note) = &event.note
        {
            log.try_insert(message_path, note.clone());
        }
        if let Some(timestamp_path) = log_schema().timestamp_key_target_path() {
            log.try_insert(timestamp_path, timestamp);
        }

        log.insert(event_path!("verb"), verb.to_string());
        log.insert(event_path!("event_uid"), uid.clone());
        if let Some(ns) = namespace.or(event.metadata.namespace.as_deref()) {
            log.insert(event_path!("namespace"), ns.to_string());
        }
        if let Some(reason) = &event.reason {
            log.insert(event_path!("reason"), reason.clone());
        }
        if let Some(type_) = &event.type_ {
            log.insert(event_path!("type"), type_.clone());
        }
        if let Some(controller) = &event.reporting_controller {
            log.insert(event_path!("reporting_controller"), controller.clone());
        }
        if let Some(instance) = &event.reporting_instance {
            log.insert(event_path!("reporting_instance"), instance.clone());
        }

        match serde_json::to_value(&event).map(|value| log.insert(event_path!("event"), value)) {
            Ok(_) => {}
            Err(error) => {
                emit!(KubernetesEventsSerializationError { error });
                return Ok(());
            }
        }

        if let (true, Some(prev)) = (self.include_previous_event, previous)
            && let Err(error) =
                serde_json::to_value(&prev).map(|value| log.insert(event_path!("old_event"), value))
        {
            emit!(KubernetesEventsSerializationError { error });
        }

        log_namespace.insert_standard_vector_source_metadata(
            &mut log,
            KubernetesEventsConfig::NAME,
            timestamp,
        );

        let byte_size = log.estimated_json_encoded_size_of();
        emit!(KubernetesEventsReceived { byte_size });

        if out.send_event(Event::from(log)).await.is_err() {
            emit!(StreamClosedError { count: 1 });
            return Err(());
        }

        Ok(())
    }

    fn type_allowed(&self, event: &KubeEvent) -> bool {
        match (&self.type_filter, &event.type_) {
            (None, _) => true,
            (Some(filter), Some(value)) => filter.contains(value),
            (Some(_), None) => false,
        }
    }

    fn reason_allowed(&self, event: &KubeEvent) -> bool {
        match (&self.reason_filter, &event.reason) {
            (None, _) => true,
            (Some(filter), Some(value)) => filter.contains(value),
            (Some(_), None) => false,
        }
    }

    fn kind_allowed(&self, event: &KubeEvent) -> bool {
        match (&self.kind_filter, &event.regarding) {
            (None, _) => true,
            (Some(filter), Some(reference)) => {
                reference.kind.as_ref().is_some_and(|k| filter.contains(k))
            }
            (Some(_), None) => false,
        }
    }

    fn is_older_than(&self, timestamp: DateTime<Utc>) -> bool {
        if self.max_event_age.is_zero() {
            return false;
        }
        match Utc::now().signed_duration_since(timestamp).to_std() {
            Ok(age) => age > self.max_event_age,
            Err(_) => false,
        }
    }
}

struct Deduper {
    entries: HashMap<String, CachedEvent>,
    retention: Duration,
}

struct CachedEvent {
    event: KubeEvent,
    resource_version: String,
    last_seen: Instant,
    timestamp: DateTime<Utc>,
}

#[derive(Debug)]
enum DedupResult {
    Added,
    Updated { previous: Option<Box<KubeEvent>> },
    Duplicate,
}

impl Deduper {
    fn new(retention: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            retention,
        }
    }

    fn record(
        &mut self,
        uid: String,
        resource_version: String,
        event: &KubeEvent,
        timestamp: DateTime<Utc>,
        include_previous: bool,
    ) -> DedupResult {
        match self.entries.get_mut(&uid) {
            Some(entry) => {
                match compare_resource_versions(&resource_version, &entry.resource_version) {
                    std::cmp::Ordering::Less | std::cmp::Ordering::Equal => DedupResult::Duplicate,
                    std::cmp::Ordering::Greater => {
                        let previous = include_previous.then(|| Box::new(entry.event.clone()));
                        entry.event = event.clone();
                        entry.resource_version = resource_version;
                        entry.last_seen = Instant::now();
                        entry.timestamp = timestamp;
                        DedupResult::Updated { previous }
                    }
                }
            }
            None => {
                self.entries.insert(
                    uid,
                    CachedEvent {
                        event: event.clone(),
                        resource_version,
                        last_seen: Instant::now(),
                        timestamp,
                    },
                );
                DedupResult::Added
            }
        }
    }

    fn prune(&mut self) {
        if self.retention.is_zero() {
            return;
        }
        let retention = self.retention;
        self.entries
            .retain(|_, entry| entry.last_seen.elapsed() <= retention);
    }

    fn remove(&mut self, uid: &str) {
        self.entries.remove(uid);
    }
}

fn compare_resource_versions(lhs: &str, rhs: &str) -> std::cmp::Ordering {
    match (lhs.parse::<u64>(), rhs.parse::<u64>()) {
        (Ok(a), Ok(b)) => a.cmp(&b),
        _ => lhs.cmp(rhs),
    }
}

fn event_timestamp(event: &KubeEvent) -> DateTime<Utc> {
    event
        .event_time
        .as_ref()
        .map(|t| t.0)
        .or_else(|| {
            event
                .series
                .as_ref()
                .map(|series| series.last_observed_time.0)
        })
        .or_else(|| event.deprecated_last_timestamp.as_ref().map(|t| t.0))
        .or_else(|| event.deprecated_first_timestamp.as_ref().map(|t| t.0))
        .or_else(|| event.metadata.creation_timestamp.as_ref().map(|t| t.0))
        .unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, TimeZone};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{MicroTime, ObjectMeta, Time};

    fn make_event(uid: &str, resource_version: &str, timestamp: DateTime<Utc>) -> KubeEvent {
        KubeEvent {
            metadata: ObjectMeta {
                uid: Some(uid.to_string()),
                resource_version: Some(resource_version.to_string()),
                ..ObjectMeta::default()
            },
            event_time: Some(MicroTime(timestamp)),
            note: Some("test".to_string()),
            ..KubeEvent::default()
        }
    }

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
    fn event_timestamp_prefers_event_time() {
        let ts = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let event = make_event("uid", "1", ts);
        assert_eq!(event_timestamp(&event), ts);
    }

    #[test]
    fn event_timestamp_falls_back_to_creation() {
        let creation_ts = Utc.timestamp_opt(1_700_000_100, 0).unwrap();
        let mut event = make_event("uid", "1", Utc::now());
        event.event_time = None;
        event.deprecated_last_timestamp = None;
        event.metadata.creation_timestamp = Some(Time(creation_ts));

        assert_eq!(event_timestamp(&event), creation_ts);
    }

    #[test]
    fn event_timestamp_uses_deprecated_fields_when_present() {
        let deprecated_ts = Utc.timestamp_opt(1_700_000_200, 0).unwrap();
        let mut event = make_event("uid", "1", Utc::now());
        event.event_time = None;
        event.deprecated_last_timestamp = Some(Time(deprecated_ts));

        assert_eq!(event_timestamp(&event), deprecated_ts);
    }
}
