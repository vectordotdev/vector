//! The watch/processing loop of the `kubernetes_events` source.

use std::{
    collections::HashSet,
    pin::Pin,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use futures::{Stream, StreamExt, stream::SelectAll};
use http_1::{HeaderName, HeaderValue};
use k8s_openapi::api::events::v1::Event as KubeEvent;
use kube::{
    Api, Client, Config as ClientConfig,
    config::{KubeConfigOptions, Kubeconfig},
    runtime::{WatchStreamExt, watcher},
};
use serde::Serialize;
use tokio::select;
use tokio::time::{Interval, MissedTickBehavior, interval};
use vector_lib::{
    config::{LegacyKey, LogNamespace, log_schema},
    internal_event::{ComponentEventsDropped, INTENTIONAL},
    lookup::{event_path, path},
};

use super::config::{self, KubernetesEventsConfig};
use super::deduper::{DedupResult, Deduper, PendingDedupeRecord};
use super::kube_timestamp_to_chrono;
use super::leader_election::{
    LeaderElectionSettings, LeadershipEnd, LeaseCoordinator, ResourceVersionCheckpoints,
    renew_leadership,
};
use super::watcher::{self as checkpoint_watcher, ApplyKind};
use crate::{
    SourceSender,
    config::{DataType, SourceConfig, SourceContext, SourceOutput},
    event::{EstimatedJsonEncodedSizeOf, Event, LogEvent},
    internal_events::{
        KubernetesEventsLeaderAcquired, KubernetesEventsLeaderLost, KubernetesEventsReceived,
        KubernetesEventsSerializationError, KubernetesEventsWatchError, StreamClosedError,
    },
    shutdown::ShutdownSignal,
};

type WatchItem = (
    String,
    Option<String>,
    Result<checkpoint_watcher::Event, checkpoint_watcher::Error>,
);
type WatchStream = Pin<Box<dyn Stream<Item = WatchItem> + Send>>;

struct EventIdentity {
    uid: String,
    resource_version: String,
}

#[derive(Serialize)]
struct CheckpointConfiguration {
    api_resource: &'static str,
    namespaces: Vec<String>,
    field_selector: Option<String>,
    label_selector: Option<String>,
    include_types: Vec<String>,
    include_reasons: Vec<String>,
    include_involved_object_kinds: Vec<String>,
    max_event_age_seconds: u64,
}

fn canonicalize_set(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn checkpoint_configuration(config: &KubernetesEventsConfig) -> String {
    let configuration = CheckpointConfiguration {
        api_resource: "events.k8s.io/v1/events",
        namespaces: canonicalize_set(config.namespaces.clone()),
        field_selector: config.field_selector.clone(),
        label_selector: config.label_selector.clone(),
        include_types: canonicalize_set(config.include_types.clone()),
        include_reasons: canonicalize_set(config.include_reasons.clone()),
        include_involved_object_kinds: canonicalize_set(
            config.include_involved_object_kinds.clone(),
        ),
        max_event_age_seconds: config.max_event_age_seconds,
    };

    serde_json::to_string(&configuration)
        .expect("checkpoint configuration contains only serializable values")
}

fn checkpoint_stream(namespace: &Option<String>) -> String {
    namespace.as_ref().map_or_else(
        || "all-namespaces".to_string(),
        |namespace| format!("namespace/{namespace}"),
    )
}

pub(super) struct KubernetesEventsSource {
    client: Client,
    namespaces: Vec<String>,
    type_filter: Option<HashSet<String>>,
    reason_filter: Option<HashSet<String>>,
    kind_filter: Option<HashSet<String>>,
    max_event_age: Duration,
    dedupe_retention: Duration,
    watcher_config: watcher::Config,
    include_previous_event: bool,
    leader_election: Option<LeaderElectionSettings>,
    checkpoint_configuration: String,
}

#[async_trait::async_trait]
#[typetag::serde(name = "kubernetes_events")]
impl SourceConfig for KubernetesEventsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let mut client_config = match &self.kube_config_file {
            Some(path) => {
                ClientConfig::from_custom_kubeconfig(
                    Kubeconfig::read_from(path)?,
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

        let source = KubernetesEventsSource::new(client, self.clone())?;

        Ok(Box::pin(source.run(cx.out, cx.shutdown, log_namespace)))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            config::schema_definition(log_namespace),
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

impl KubernetesEventsSource {
    fn new(client: Client, config: KubernetesEventsConfig) -> crate::Result<Self> {
        if config.dedupe_retention_seconds == 0 {
            return Err("dedupe_retention_seconds must be greater than 0".into());
        }
        if !(1..=config::MAX_WATCH_TIMEOUT_SECS).contains(&config.watch_timeout_seconds) {
            return Err(format!(
                "watch_timeout_seconds must be between 1 and {}",
                config::MAX_WATCH_TIMEOUT_SECS
            )
            .into());
        }

        let checkpoint_configuration = checkpoint_configuration(&config);
        let namespaces = canonicalize_set(config.namespaces.clone());
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

        Ok(Self {
            client,
            namespaces,
            type_filter,
            reason_filter,
            kind_filter,
            max_event_age: Duration::from_secs(config.max_event_age_seconds),
            dedupe_retention: Duration::from_secs(config.dedupe_retention_seconds),
            watcher_config,
            include_previous_event: config.include_previous_event,
            leader_election: LeaderElectionSettings::from_config(&config.leader_election)?,
            checkpoint_configuration,
        })
    }

    fn build_streams(
        &self,
        checkpoints: Option<&ResourceVersionCheckpoints>,
    ) -> SelectAll<WatchStream> {
        let mut streams = SelectAll::new();

        if self.namespaces.is_empty() {
            let api: Api<KubeEvent> = Api::all(self.client.clone());
            streams.push(self.make_stream(api, None, checkpoints));
        } else {
            for namespace in &self.namespaces {
                let api: Api<KubeEvent> = Api::namespaced(self.client.clone(), namespace);
                streams.push(self.make_stream(api, Some(namespace.clone()), checkpoints));
            }
        }

        streams
    }

    fn make_stream(
        &self,
        api: Api<KubeEvent>,
        namespace: Option<String>,
        checkpoints: Option<&ResourceVersionCheckpoints>,
    ) -> WatchStream {
        let cfg = self.watcher_config.clone();
        let stream = checkpoint_stream(&namespace);
        let initial_resource_version = checkpoints
            .and_then(|checkpoints| checkpoints.get(&stream))
            .map(ToString::to_string);
        Box::pin(
            checkpoint_watcher::resumable_watcher(api, cfg, initial_resource_version)
                .default_backoff()
                .map(move |event| (stream.clone(), namespace.clone(), event)),
        )
    }

    async fn run(
        mut self,
        mut out: SourceSender,
        mut shutdown: ShutdownSignal,
        log_namespace: LogNamespace,
    ) -> Result<(), ()> {
        let mut deduper = Deduper::new(self.dedupe_retention);

        if let Some(settings) = self.leader_election.clone() {
            return self
                .run_with_leader_election(
                    &mut out,
                    &mut shutdown,
                    log_namespace,
                    &mut deduper,
                    settings,
                )
                .await;
        }

        self.run_active(&mut out, &mut shutdown, log_namespace, &mut deduper)
            .await
    }

    async fn run_active(
        &mut self,
        out: &mut SourceSender,
        shutdown: &mut ShutdownSignal,
        log_namespace: LogNamespace,
        deduper: &mut Deduper,
    ) -> Result<(), ()> {
        let mut streams = self.build_streams(None);

        loop {
            select! {
                _ = &mut *shutdown => break,
                maybe_event = streams.next() => {
                    match maybe_event {
                        Some((_, namespace, Ok(event))) => {
                            if let Some(processed) =
                                self.handle_event(
                                    namespace.as_deref(),
                                    event,
                                    log_namespace,
                                    deduper,
                                )?
                            {
                                let dedupe_record = processed.dedupe_record;
                                if send_event(out, processed.event).await.is_err() {
                                    return Err(());
                                }
                                deduper.commit(dedupe_record);
                            }
                        }
                        Some((_, _, Err(error))) => {
                            emit!(KubernetesEventsWatchError { error });
                        }
                        None => break,
                    }
                }
            }
        }

        Ok(())
    }

    async fn run_with_leader_election(
        &mut self,
        out: &mut SourceSender,
        shutdown: &mut ShutdownSignal,
        log_namespace: LogNamespace,
        deduper: &mut Deduper,
        settings: LeaderElectionSettings,
    ) -> Result<(), ()> {
        let coordinator = LeaseCoordinator::new(self.client.clone(), settings);

        loop {
            let Some(acquired) = coordinator
                .wait_for_leadership(shutdown, &self.checkpoint_configuration)
                .await
            else {
                break;
            };

            emit!(KubernetesEventsLeaderAcquired {
                identity: coordinator.settings.identity.clone(),
                lease_namespace: coordinator.settings.lease_namespace.clone(),
                lease_name: coordinator.settings.lease_name.clone(),
                checkpoint_streams: acquired.checkpoints.len(),
            });

            match self
                .run_leadership_epoch(
                    out,
                    shutdown,
                    log_namespace,
                    deduper,
                    &coordinator,
                    acquired.checkpoints,
                )
                .await?
            {
                LeadershipEnd::Shutdown => break,
                LeadershipEnd::RestartWatch => {}
                LeadershipEnd::Lost(reason) => {
                    deduper.invalidate_previous_events();
                    emit!(KubernetesEventsLeaderLost {
                        identity: coordinator.settings.identity.clone(),
                        reason,
                    });
                }
            }
        }

        Ok(())
    }

    async fn run_leadership_epoch(
        &mut self,
        out: &mut SourceSender,
        shutdown: &mut ShutdownSignal,
        log_namespace: LogNamespace,
        deduper: &mut Deduper,
        coordinator: &LeaseCoordinator,
        mut checkpoints: ResourceVersionCheckpoints,
    ) -> Result<LeadershipEnd, ()> {
        let mut streams = self.build_streams(Some(&checkpoints));
        let mut renew_interval = interval(coordinator.settings.retry_period);
        renew_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut last_renewal = Instant::now();

        loop {
            select! {
                _ = &mut *shutdown => return Ok(LeadershipEnd::Shutdown),
                _ = renew_interval.tick() => {
                    if let Some(end) =
                        renew_leadership(coordinator, &mut last_renewal, &checkpoints).await
                    {
                        return Ok(end);
                    }
                }
                maybe_event = streams.next() => {
                    match maybe_event {
                        Some((stream, namespace, Ok(event))) => {
                            let checkpoint = event.checkpoint().map(ToString::to_string);
                            if let Some(processed) =
                                self.handle_event(
                                    namespace.as_deref(),
                                    event,
                                    log_namespace,
                                    deduper,
                                )?
                            {
                                let dedupe_record = processed.dedupe_record;
                                if let Some(end) = send_event_with_leadership(
                                    out,
                                    processed.event,
                                    shutdown,
                                    &mut renew_interval,
                                    &mut last_renewal,
                                    coordinator,
                                    &checkpoints,
                                )
                                .await?
                                {
                                    return Ok(end);
                                }
                                deduper.commit(dedupe_record);
                            }
                            if let Some(resource_version) = checkpoint {
                                checkpoints.set(stream, resource_version);
                            }
                        }
                        Some((_, _, Err(error))) => {
                            emit!(KubernetesEventsWatchError { error });
                        }
                        None => return Ok(LeadershipEnd::RestartWatch),
                    }
                }
            }
        }
    }

    fn handle_event(
        &mut self,
        namespace: Option<&str>,
        event: checkpoint_watcher::Event,
        log_namespace: LogNamespace,
        deduper: &mut Deduper,
    ) -> Result<Option<ProcessedEvent>, ()> {
        match event {
            checkpoint_watcher::Event::Apply { event, kind } => {
                self.process_apply_event(namespace, event, log_namespace, deduper, Some(kind))
            }
            checkpoint_watcher::Event::InitApply(event) => {
                self.process_apply_event(namespace, event, log_namespace, deduper, None)
            }
            checkpoint_watcher::Event::Delete(event) => {
                if let Some(uid) = event.metadata.uid.as_deref() {
                    deduper.remove(uid);
                }
                Ok(None)
            }
            checkpoint_watcher::Event::Init => Ok(None),
            checkpoint_watcher::Event::InitDone { .. } => {
                deduper.prune();
                Ok(None)
            }
            checkpoint_watcher::Event::Bookmark { .. } => Ok(None),
        }
    }

    fn process_apply_event(
        &mut self,
        namespace: Option<&str>,
        event: KubeEvent,
        log_namespace: LogNamespace,
        deduper: &mut Deduper,
        apply_kind: Option<ApplyKind>,
    ) -> Result<Option<ProcessedEvent>, ()> {
        let Some(identity) = event_identity(&event) else {
            return Ok(None);
        };
        let uid = identity.uid;
        let resource_version = identity.resource_version;
        deduper.prune();

        if !self.type_allowed(&event) || !self.reason_allowed(&event) || !self.kind_allowed(&event)
        {
            if self.include_previous_event {
                deduper.commit(PendingDedupeRecord {
                    uid,
                    resource_version,
                    event: Some(event),
                });
            }
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "filtered"
            });
            return Ok(None);
        }

        let observed_timestamp = observed_event_timestamp(&event);
        let timestamp = observed_timestamp.unwrap_or_else(Utc::now);
        if self.is_older_than(timestamp) {
            if self.include_previous_event {
                deduper.commit(PendingDedupeRecord {
                    uid,
                    resource_version,
                    event: Some(event),
                });
            }
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "expired"
            });
            return Ok(None);
        }

        let dedupe_record = PendingDedupeRecord {
            uid: uid.clone(),
            resource_version: resource_version.clone(),
            event: self.include_previous_event.then(|| event.clone()),
        };

        let dedup_result = deduper.evaluate(&uid, &resource_version, self.include_previous_event);

        let (verb, previous) = match dedup_result {
            DedupResult::Duplicate => {
                emit!(ComponentEventsDropped::<INTENTIONAL> {
                    count: 1,
                    reason: "duplicate"
                });
                return Ok(None);
            }
            DedupResult::Added => (
                if apply_kind == Some(ApplyKind::Modified) {
                    "UPDATED"
                } else {
                    "ADDED"
                },
                None,
            ),
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

        let event_namespace = namespace.or(event.metadata.namespace.as_deref());
        insert_kubernetes_events_metadata(
            log_namespace,
            &mut log,
            KubernetesEventMetadata {
                verb,
                uid: &uid,
                namespace: event_namespace,
                reason: event.reason.as_deref(),
                type_: event.type_.as_deref(),
                received_at: Utc::now(),
            },
        );
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
                return Ok(None);
            }
        }

        if let (true, Some(prev)) = (self.include_previous_event, previous)
            && let Err(error) =
                serde_json::to_value(&prev).map(|value| log.insert(event_path!("old_event"), value))
        {
            emit!(KubernetesEventsSerializationError { error });
        }

        let byte_size = log.estimated_json_encoded_size_of();
        emit!(KubernetesEventsReceived { byte_size });

        Ok(Some(ProcessedEvent {
            event: Event::from(log),
            dedupe_record,
        }))
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

struct KubernetesEventMetadata<'a> {
    verb: &'a str,
    uid: &'a str,
    namespace: Option<&'a str>,
    reason: Option<&'a str>,
    type_: Option<&'a str>,
    received_at: DateTime<Utc>,
}

fn insert_kubernetes_events_metadata(
    log_namespace: LogNamespace,
    log: &mut LogEvent,
    metadata: KubernetesEventMetadata<'_>,
) {
    log_namespace.insert_source_metadata(
        KubernetesEventsConfig::NAME,
        log,
        Some(LegacyKey::InsertIfEmpty(path!("verb"))),
        path!("verb"),
        metadata.verb,
    );
    log_namespace.insert_source_metadata(
        KubernetesEventsConfig::NAME,
        log,
        Some(LegacyKey::InsertIfEmpty(path!("event_uid"))),
        path!("event_uid"),
        metadata.uid,
    );
    if let Some(namespace) = metadata.namespace {
        log_namespace.insert_source_metadata(
            KubernetesEventsConfig::NAME,
            log,
            Some(LegacyKey::InsertIfEmpty(path!("namespace"))),
            path!("namespace"),
            namespace,
        );
    }
    if let Some(reason) = metadata.reason {
        log_namespace.insert_source_metadata(
            KubernetesEventsConfig::NAME,
            log,
            Some(LegacyKey::InsertIfEmpty(path!("reason"))),
            path!("reason"),
            reason,
        );
    }
    if let Some(type_) = metadata.type_ {
        log_namespace.insert_source_metadata(
            KubernetesEventsConfig::NAME,
            log,
            Some(LegacyKey::InsertIfEmpty(path!("type"))),
            path!("type"),
            type_,
        );
    }
    log_namespace.insert_standard_vector_source_metadata(
        log,
        KubernetesEventsConfig::NAME,
        metadata.received_at,
    );
}

struct ProcessedEvent {
    event: Event,
    dedupe_record: PendingDedupeRecord,
}

async fn send_event(out: &mut SourceSender, event: Event) -> Result<(), ()> {
    if out.send_event(event).await.is_err() {
        emit!(StreamClosedError { count: 1 });
        return Err(());
    }

    Ok(())
}

async fn send_event_with_leadership(
    out: &mut SourceSender,
    event: Event,
    shutdown: &mut ShutdownSignal,
    renew_interval: &mut Interval,
    last_renewal: &mut Instant,
    coordinator: &LeaseCoordinator,
    checkpoints: &ResourceVersionCheckpoints,
) -> Result<Option<LeadershipEnd>, ()> {
    let send = out.send_event(event);
    tokio::pin!(send);

    loop {
        select! {
            _ = &mut *shutdown => return Ok(Some(LeadershipEnd::Shutdown)),
            result = &mut send => {
                if result.is_err() {
                    emit!(StreamClosedError { count: 1 });
                    return Err(());
                }
                return Ok(None);
            }
            _ = renew_interval.tick() => {
                if let Some(end) = renew_leadership(coordinator, last_renewal, checkpoints).await {
                    return Ok(Some(end));
                }
            }
        }
    }
}

fn observed_event_timestamp(event: &KubeEvent) -> Option<DateTime<Utc>> {
    event
        .series
        .as_ref()
        .map(|series| series.last_observed_time.0)
        .or_else(|| event.event_time.as_ref().map(|t| t.0))
        .or_else(|| event.deprecated_last_timestamp.as_ref().map(|t| t.0))
        .or_else(|| event.deprecated_first_timestamp.as_ref().map(|t| t.0))
        .or_else(|| event.metadata.creation_timestamp.as_ref().map(|t| t.0))
        .and_then(kube_timestamp_to_chrono)
}

#[cfg(test)]
fn event_timestamp(event: &KubeEvent) -> DateTime<Utc> {
    observed_event_timestamp(event).unwrap_or_else(Utc::now)
}

fn event_identity(event: &KubeEvent) -> Option<EventIdentity> {
    let uid = match event.metadata.uid.clone() {
        Some(uid) => uid,
        None => {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "missing_uid"
            });
            return None;
        }
    };

    let resource_version = match event.metadata.resource_version.clone() {
        Some(resource_version) => resource_version,
        None => {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "missing_resource_version"
            });
            return None;
        }
    };

    Some(EventIdentity {
        uid,
        resource_version,
    })
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{kube_timestamp, make_event};
    use super::*;
    use chrono::{Duration as ChronoDuration, TimeZone};
    use k8s_openapi::api::events::v1::EventSeries;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{MicroTime, Time};
    use vrl::value;

    fn make_source() -> KubernetesEventsSource {
        make_source_with_config(KubernetesEventsConfig::default())
    }

    fn make_source_with_config(config: KubernetesEventsConfig) -> KubernetesEventsSource {
        let client_config = ClientConfig::new("http://127.0.0.1:8080".parse().unwrap());
        let client = Client::try_from(client_config).unwrap();
        KubernetesEventsSource::new(client, config).unwrap()
    }

    #[test]
    fn inserts_kubernetes_event_metadata_in_vector_namespace() {
        let mut log = LogEvent::default();
        let received_at = Utc.timestamp_opt(1_700_000_500, 0).unwrap();

        insert_kubernetes_events_metadata(
            LogNamespace::Vector,
            &mut log,
            KubernetesEventMetadata {
                verb: "ADDED",
                uid: "event-uid",
                namespace: Some("kube-system"),
                reason: Some("FailedScheduling"),
                type_: Some("Warning"),
                received_at,
            },
        );

        let meta = log.metadata().value();
        assert_eq!(
            meta.get(path!(KubernetesEventsConfig::NAME, "verb")),
            Some(&value!("ADDED"))
        );
        assert_eq!(
            meta.get(path!(KubernetesEventsConfig::NAME, "event_uid")),
            Some(&value!("event-uid"))
        );
        assert_eq!(
            meta.get(path!(KubernetesEventsConfig::NAME, "namespace")),
            Some(&value!("kube-system"))
        );
        assert_eq!(
            meta.get(path!(KubernetesEventsConfig::NAME, "reason")),
            Some(&value!("FailedScheduling"))
        );
        assert_eq!(
            meta.get(path!(KubernetesEventsConfig::NAME, "type")),
            Some(&value!("Warning"))
        );
        assert_eq!(
            meta.get(path!("vector", "source_type")),
            Some(&value!(KubernetesEventsConfig::NAME))
        );
        assert_eq!(
            meta.get(path!("vector", "ingest_timestamp")),
            Some(&value!(received_at))
        );

        assert!(log.value().get(path!("verb")).is_none());
        assert!(log.value().get(path!("event_uid")).is_none());
        assert!(log.value().get(path!("namespace")).is_none());
        assert!(log.value().get(path!("reason")).is_none());
        assert!(log.value().get(path!("type")).is_none());
    }

    #[test]
    fn inserts_kubernetes_event_metadata_in_legacy_namespace() {
        let mut log = LogEvent::default();
        let event_timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let received_at = event_timestamp + ChronoDuration::seconds(500);

        log.insert(event_path!("timestamp"), event_timestamp);
        insert_kubernetes_events_metadata(
            LogNamespace::Legacy,
            &mut log,
            KubernetesEventMetadata {
                verb: "UPDATED",
                uid: "event-uid",
                namespace: Some("default"),
                reason: Some("BackOff"),
                type_: Some("Normal"),
                received_at,
            },
        );

        assert_eq!(log.value().get(path!("verb")), Some(&value!("UPDATED")));
        assert_eq!(
            log.value().get(path!("event_uid")),
            Some(&value!("event-uid"))
        );
        assert_eq!(
            log.value().get(path!("namespace")),
            Some(&value!("default"))
        );
        assert_eq!(log.value().get(path!("reason")), Some(&value!("BackOff")));
        assert_eq!(log.value().get(path!("type")), Some(&value!("Normal")));
        assert_eq!(
            log.value().get(path!("source_type")),
            Some(&value!(KubernetesEventsConfig::NAME))
        );
        assert_eq!(
            log.value().get(path!("timestamp")),
            Some(&value!(event_timestamp))
        );
    }

    #[tokio::test]
    async fn leader_bootstrap_init_apply_uses_normal_dedupe() {
        let mut source = make_source();
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let event = make_event("uid", "rv", Utc::now() - ChronoDuration::seconds(10));

        let processed = source
            .handle_event(
                None,
                checkpoint_watcher::Event::InitApply(event),
                LogNamespace::Legacy,
                &mut deduper,
            )
            .unwrap();

        assert!(
            processed.is_some(),
            "bootstrap events should be emitted rather than suppressed by lease timing"
        );
        let dedupe_record = processed.unwrap().dedupe_record;
        deduper.commit(dedupe_record);

        let replayed_event = make_event("uid", "rv", Utc::now() - ChronoDuration::seconds(10));
        let processed = source
            .handle_event(
                None,
                checkpoint_watcher::Event::InitApply(replayed_event),
                LogNamespace::Legacy,
                &mut deduper,
            )
            .unwrap();

        assert!(
            processed.is_none(),
            "already delivered bootstrap events should still be suppressed by dedupe"
        );
    }

    #[test]
    fn event_timestamp_prefers_series_last_observed_time() {
        let ts = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let last_observed_ts = ts + ChronoDuration::seconds(10);
        let mut event = make_event("uid", "1", ts);
        event.series = Some(EventSeries {
            count: 2,
            last_observed_time: MicroTime(kube_timestamp(last_observed_ts)),
        });

        assert_eq!(event_timestamp(&event), last_observed_ts);
    }

    #[test]
    fn event_timestamp_falls_back_to_creation() {
        let creation_ts = Utc.timestamp_opt(1_700_000_100, 0).unwrap();
        let mut event = make_event("uid", "1", Utc::now());
        event.event_time = None;
        event.deprecated_last_timestamp = None;
        event.metadata.creation_timestamp = Some(Time(kube_timestamp(creation_ts)));

        assert_eq!(event_timestamp(&event), creation_ts);
    }

    #[test]
    fn event_timestamp_uses_deprecated_fields_when_present() {
        let deprecated_ts = Utc.timestamp_opt(1_700_000_200, 0).unwrap();
        let mut event = make_event("uid", "1", Utc::now());
        event.event_time = None;
        event.deprecated_last_timestamp = Some(Time(kube_timestamp(deprecated_ts)));

        assert_eq!(event_timestamp(&event), deprecated_ts);
    }

    #[test]
    fn event_timestamp_prefers_event_time_over_deprecated_last_timestamp() {
        let event_ts = Utc.timestamp_opt(1_700_000_300, 0).unwrap();
        let deprecated_ts = event_ts - ChronoDuration::seconds(30);
        let mut event = make_event("uid", "1", event_ts);
        event.deprecated_last_timestamp = Some(Time(kube_timestamp(deprecated_ts)));

        assert_eq!(event_timestamp(&event), event_ts);
    }

    #[tokio::test]
    async fn bootstrap_events_are_never_suppressed_by_event_timestamp() {
        let mut source = make_source();
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let delayed = make_event(
            "uid-delayed",
            "rv1",
            Utc::now() - ChronoDuration::minutes(30),
        );
        let processed = source
            .handle_event(
                None,
                checkpoint_watcher::Event::InitApply(delayed),
                LogNamespace::Legacy,
                &mut deduper,
            )
            .unwrap();

        assert!(
            processed.is_some(),
            "event timestamps cannot prove whether an initial-list object was delivered"
        );
    }

    #[tokio::test]
    async fn modified_watch_event_is_updated_without_local_dedupe_state() {
        let mut source = make_source();
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let event = make_event("uid", "rv", Utc::now());
        let processed = source
            .handle_event(
                None,
                checkpoint_watcher::Event::Apply {
                    event,
                    kind: ApplyKind::Modified,
                },
                LogNamespace::Legacy,
                &mut deduper,
            )
            .unwrap()
            .expect("modified event should be emitted");

        assert_eq!(
            processed.event.as_log().value().get(path!("verb")),
            Some(&value!("UPDATED"))
        );
    }

    #[tokio::test]
    async fn caches_event_payload_only_when_previous_event_is_enabled() {
        for include_previous_event in [false, true] {
            let mut source = make_source_with_config(KubernetesEventsConfig {
                include_previous_event,
                ..KubernetesEventsConfig::default()
            });
            let mut deduper = Deduper::new(Duration::from_secs(60));
            let processed = source
                .handle_event(
                    None,
                    checkpoint_watcher::Event::InitApply(make_event("uid", "rv", Utc::now())),
                    LogNamespace::Legacy,
                    &mut deduper,
                )
                .unwrap()
                .expect("event should be emitted");

            assert_eq!(
                processed.dedupe_record.event.is_some(),
                include_previous_event
            );
            deduper.commit(processed.dedupe_record);
            assert!(matches!(
                deduper.evaluate("uid", "rv", include_previous_event),
                DedupResult::Duplicate
            ));
        }
    }

    #[tokio::test]
    async fn filtered_versions_are_retained_for_previous_event_output() {
        let mut source = make_source_with_config(KubernetesEventsConfig {
            include_types: vec!["Normal".to_string()],
            include_previous_event: true,
            ..KubernetesEventsConfig::default()
        });
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let mut first = make_event("uid", "1", Utc::now());
        first.type_ = Some("Normal".to_string());
        let first = source
            .handle_event(
                None,
                checkpoint_watcher::Event::InitApply(first),
                LogNamespace::Legacy,
                &mut deduper,
            )
            .unwrap()
            .expect("matching event should be emitted");
        deduper.commit(first.dedupe_record);

        let mut filtered = make_event("uid", "2", Utc::now());
        filtered.type_ = Some("Warning".to_string());
        assert!(
            source
                .handle_event(
                    None,
                    checkpoint_watcher::Event::InitApply(filtered),
                    LogNamespace::Legacy,
                    &mut deduper,
                )
                .unwrap()
                .is_none(),
            "filtered event should not be emitted"
        );

        let mut latest = make_event("uid", "3", Utc::now());
        latest.type_ = Some("Normal".to_string());
        let latest = source
            .handle_event(
                None,
                checkpoint_watcher::Event::InitApply(latest),
                LogNamespace::Legacy,
                &mut deduper,
            )
            .unwrap()
            .expect("event matching the filter again should be emitted");

        assert_eq!(
            latest
                .event
                .as_log()
                .value()
                .get(path!("old_event", "metadata", "resourceVersion")),
            Some(&value!("2")),
            "old_event should contain the most recently observed version"
        );
    }

    #[tokio::test]
    async fn bootstrap_event_without_timestamp_is_emitted() {
        let mut source = make_source();
        let mut deduper = Deduper::new(Duration::from_secs(60));

        let mut event = make_event("uid", "rv", Utc::now());
        event.event_time = None;
        assert!(observed_event_timestamp(&event).is_none());

        let processed = source
            .handle_event(
                None,
                checkpoint_watcher::Event::InitApply(event),
                LogNamespace::Legacy,
                &mut deduper,
            )
            .unwrap();

        assert!(
            processed.is_some(),
            "events without an observable timestamp should still be handled"
        );
    }

    #[test]
    fn checkpoint_configuration_is_canonical_for_set_like_fields() {
        let first = KubernetesEventsConfig {
            namespaces: vec!["b".to_string(), "a".to_string()],
            include_types: vec!["Warning".to_string(), "Normal".to_string()],
            ..KubernetesEventsConfig::default()
        };

        let second = KubernetesEventsConfig {
            namespaces: vec!["a".to_string(), "b".to_string()],
            include_types: vec!["Normal".to_string(), "Warning".to_string()],
            ..KubernetesEventsConfig::default()
        };

        assert_eq!(
            checkpoint_configuration(&first),
            checkpoint_configuration(&second)
        );
    }

    #[tokio::test]
    async fn runtime_namespaces_are_sorted_and_deduplicated() {
        let source = make_source_with_config(KubernetesEventsConfig {
            namespaces: vec!["b".to_string(), "a".to_string(), "a".to_string()],
            ..KubernetesEventsConfig::default()
        });

        assert_eq!(source.namespaces, ["a", "b"]);
    }

    #[tokio::test]
    async fn zero_dedupe_retention_is_rejected() {
        let client_config = ClientConfig::new("http://127.0.0.1:8080".parse().unwrap());
        let client = Client::try_from(client_config).unwrap();
        let result = KubernetesEventsSource::new(
            client,
            KubernetesEventsConfig {
                dedupe_retention_seconds: 0,
                ..KubernetesEventsConfig::default()
            },
        );

        assert!(matches!(
            result,
            Err(error) if error.to_string() == "dedupe_retention_seconds must be greater than 0"
        ));
    }

    #[tokio::test]
    async fn invalid_watch_timeouts_are_rejected() {
        let client_config = ClientConfig::new("http://127.0.0.1:8080".parse().unwrap());
        let client = Client::try_from(client_config).unwrap();

        for watch_timeout_seconds in [0, config::MAX_WATCH_TIMEOUT_SECS + 1] {
            let result = KubernetesEventsSource::new(
                client.clone(),
                KubernetesEventsConfig {
                    watch_timeout_seconds,
                    ..KubernetesEventsConfig::default()
                },
            );

            assert!(matches!(
                result,
                Err(error) if error.to_string()
                    == format!(
                        "watch_timeout_seconds must be between 1 and {}",
                        config::MAX_WATCH_TIMEOUT_SECS
                    )
            ));
        }

        assert!(
            KubernetesEventsSource::new(
                client,
                KubernetesEventsConfig {
                    watch_timeout_seconds: config::MAX_WATCH_TIMEOUT_SECS,
                    ..KubernetesEventsConfig::default()
                },
            )
            .is_ok()
        );
    }
}
