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
    LeaderElectionSettings, LeadershipEnd, LeaseCoordinator, max_watermark, renew_leadership,
    replay_cutoff,
};
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

type WatchItem = (Option<String>, watcher::Result<watcher::Event<KubeEvent>>);
type WatchStream = Pin<Box<dyn Stream<Item = WatchItem> + Send>>;

struct EventIdentity {
    uid: String,
    resource_version: String,
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
            namespaces: config.namespaces.clone(),
            type_filter,
            reason_filter,
            kind_filter,
            max_event_age: Duration::from_secs(config.max_event_age_seconds),
            dedupe_retention: Duration::from_secs(config.dedupe_retention_seconds),
            watcher_config,
            include_previous_event: config.include_previous_event,
            leader_election: LeaderElectionSettings::from_config(&config.leader_election)?,
        })
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
        Box::pin(
            watcher(api, cfg)
                .backoff(watcher::DefaultBackoff::default())
                .map(move |event| (namespace.clone(), event)),
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
        let mut streams = self.build_streams();

        loop {
            select! {
                _ = &mut *shutdown => break,
                maybe_event = streams.next() => {
                    match maybe_event {
                        Some((namespace, Ok(event))) => {
                            if let Some(processed) =
                                self.handle_event(
                                    namespace.as_deref(),
                                    event,
                                    log_namespace,
                                    deduper,
                                    None,
                                )?
                            {
                                let dedupe_record = processed.dedupe_record;
                                if send_event(out, processed.event).await.is_err() {
                                    return Err(());
                                }
                                deduper.commit(dedupe_record);
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
            let Some(acquired) = coordinator.wait_for_leadership(shutdown).await else {
                break;
            };

            emit!(KubernetesEventsLeaderAcquired {
                identity: coordinator.settings.identity.clone(),
                lease_namespace: coordinator.settings.lease_namespace.clone(),
                lease_name: coordinator.settings.lease_name.clone(),
                resume_watermark: acquired.watermark,
            });

            match self
                .run_leadership_epoch(
                    out,
                    shutdown,
                    log_namespace,
                    deduper,
                    &coordinator,
                    acquired.watermark,
                )
                .await?
            {
                LeadershipEnd::Shutdown => break,
                LeadershipEnd::RestartWatch => {}
                LeadershipEnd::Lost(reason) => emit!(KubernetesEventsLeaderLost {
                    identity: coordinator.settings.identity.clone(),
                    reason,
                }),
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
        acquired_watermark: Option<DateTime<Utc>>,
    ) -> Result<LeadershipEnd, ()> {
        // Replays are filtered against the watermark observed at acquisition, not the advancing
        // one: an event first seen through a mid-epoch relist may be older than events this
        // replica already forwarded, and must not be dropped.
        let replay_cutoff = replay_cutoff(acquired_watermark, coordinator.settings.watermark_grace);
        let mut watermark = acquired_watermark;
        let mut streams = self.build_streams();
        let mut renew_interval = interval(coordinator.settings.retry_period);
        renew_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut last_renewal = Instant::now();

        loop {
            select! {
                _ = &mut *shutdown => return Ok(LeadershipEnd::Shutdown),
                _ = renew_interval.tick() => {
                    if let Some(end) =
                        renew_leadership(coordinator, &mut last_renewal, watermark).await
                    {
                        return Ok(end);
                    }
                }
                maybe_event = streams.next() => {
                    match maybe_event {
                        Some((namespace, Ok(event))) => {
                            if let Some(processed) =
                                self.handle_event(
                                    namespace.as_deref(),
                                    event,
                                    log_namespace,
                                    deduper,
                                    replay_cutoff,
                                )?
                            {
                                let dedupe_record = processed.dedupe_record;
                                let event_timestamp = processed.timestamp;
                                if let Some(end) = send_event_with_leadership(
                                    out,
                                    processed.event,
                                    shutdown,
                                    &mut renew_interval,
                                    &mut last_renewal,
                                    coordinator,
                                    watermark,
                                )
                                .await?
                                {
                                    return Ok(end);
                                }
                                deduper.commit(dedupe_record);
                                watermark = max_watermark(watermark, event_timestamp);
                            }
                        }
                        Some((_, Err(error))) => {
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
        event: watcher::Event<KubeEvent>,
        log_namespace: LogNamespace,
        deduper: &mut Deduper,
        replay_cutoff: Option<DateTime<Utc>>,
    ) -> Result<Option<ProcessedEvent>, ()> {
        match event {
            watcher::Event::Apply(ev) => {
                // Live watch events are new writes to the API server; the watermark only applies
                // to the initial list, which replays state a previous leader already forwarded.
                self.process_apply_event(namespace, ev, log_namespace, deduper, None)
            }
            watcher::Event::InitApply(ev) => {
                self.process_apply_event(namespace, ev, log_namespace, deduper, replay_cutoff)
            }
            watcher::Event::Delete(ev) => {
                if let Some(uid) = ev.metadata.uid.as_deref() {
                    deduper.remove(uid);
                }
                Ok(None)
            }
            watcher::Event::Init => Ok(None),
            watcher::Event::InitDone => {
                deduper.prune();
                Ok(None)
            }
        }
    }

    fn process_apply_event(
        &mut self,
        namespace: Option<&str>,
        event: KubeEvent,
        log_namespace: LogNamespace,
        deduper: &mut Deduper,
        replay_cutoff: Option<DateTime<Utc>>,
    ) -> Result<Option<ProcessedEvent>, ()> {
        let Some(identity) = event_identity(&event) else {
            return Ok(None);
        };
        let uid = identity.uid;
        let resource_version = identity.resource_version;

        if !self.type_allowed(&event) || !self.reason_allowed(&event) || !self.kind_allowed(&event)
        {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "filtered"
            });
            return Ok(None);
        }

        let observed_timestamp = observed_event_timestamp(&event);
        let timestamp = observed_timestamp.unwrap_or_else(Utc::now);
        if self.is_older_than(timestamp) {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "expired"
            });
            return Ok(None);
        }

        // Events without an observable timestamp are never dropped here; the deduper is the only
        // protection against replaying those.
        if let (Some(cutoff), Some(observed)) = (replay_cutoff, observed_timestamp)
            && observed <= cutoff
        {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "already_forwarded"
            });
            return Ok(None);
        }

        deduper.prune();

        let dedupe_record = PendingDedupeRecord {
            uid: uid.clone(),
            resource_version: resource_version.clone(),
            event: event.clone(),
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
            timestamp: observed_timestamp,
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
    /// The observable timestamp used to advance the delivery watermark; `None` when the event
    /// carries no usable timestamp.
    timestamp: Option<DateTime<Utc>>,
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
    watermark: Option<DateTime<Utc>>,
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
                if let Some(end) = renew_leadership(coordinator, last_renewal, watermark).await {
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
                watcher::Event::InitApply(event),
                LogNamespace::Legacy,
                &mut deduper,
                None,
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
                watcher::Event::InitApply(replayed_event),
                LogNamespace::Legacy,
                &mut deduper,
                None,
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
    async fn bootstrap_replays_below_watermark_are_dropped() {
        let mut source = make_source();
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let cutoff = Utc::now() - ChronoDuration::seconds(60);

        let already_sent = make_event("uid-old", "rv1", cutoff - ChronoDuration::seconds(30));
        let processed = source
            .handle_event(
                None,
                watcher::Event::InitApply(already_sent),
                LogNamespace::Legacy,
                &mut deduper,
                Some(cutoff),
            )
            .unwrap();
        assert!(
            processed.is_none(),
            "initial-list events at or below the watermark cutoff should be skipped"
        );

        let fresh = make_event("uid-new", "rv2", cutoff + ChronoDuration::seconds(30));
        let processed = source
            .handle_event(
                None,
                watcher::Event::InitApply(fresh),
                LogNamespace::Legacy,
                &mut deduper,
                Some(cutoff),
            )
            .unwrap();
        assert!(
            processed.is_some(),
            "initial-list events newer than the cutoff should be emitted"
        );
    }

    #[tokio::test]
    async fn bootstrap_replay_at_exact_watermark_is_dropped() {
        let mut source = make_source();
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let cutoff = Utc::now() - ChronoDuration::seconds(60);

        let at_watermark = make_event("uid", "rv", cutoff);
        let processed = source
            .handle_event(
                None,
                watcher::Event::InitApply(at_watermark),
                LogNamespace::Legacy,
                &mut deduper,
                Some(cutoff),
            )
            .unwrap();

        assert!(
            processed.is_none(),
            "an event exactly at the cutoff was already forwarded and should be skipped"
        );
    }

    #[tokio::test]
    async fn live_apply_events_ignore_watermark() {
        let mut source = make_source();
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let cutoff = Utc::now() - ChronoDuration::seconds(60);

        let live = make_event("uid", "rv", cutoff - ChronoDuration::seconds(30));
        let processed = source
            .handle_event(
                None,
                watcher::Event::Apply(live),
                LogNamespace::Legacy,
                &mut deduper,
                Some(cutoff),
            )
            .unwrap();

        assert!(
            processed.is_some(),
            "live watch events are new API writes and must not be filtered by the watermark"
        );
    }

    #[tokio::test]
    async fn bootstrap_replay_without_timestamp_is_not_dropped() {
        let mut source = make_source();
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let cutoff = Utc::now() - ChronoDuration::seconds(60);

        let mut event = make_event("uid", "rv", cutoff - ChronoDuration::seconds(30));
        event.event_time = None;
        assert!(observed_event_timestamp(&event).is_none());

        let processed = source
            .handle_event(
                None,
                watcher::Event::InitApply(event),
                LogNamespace::Legacy,
                &mut deduper,
                Some(cutoff),
            )
            .unwrap();

        assert!(
            processed.is_some(),
            "events without an observable timestamp cannot be proven already-forwarded"
        );
        assert!(
            processed.unwrap().timestamp.is_none(),
            "timestampless events must not advance the delivery watermark"
        );
    }
}
