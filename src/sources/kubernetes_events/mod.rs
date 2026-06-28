#![deny(missing_docs)]

//! Kubernetes events source.
//!
//! This source watches the Kubernetes Events API and emits each event as a Vector log event. It is
//! designed for singleton deployments that run once per cluster.

mod config;

use config::{
    FALLBACK_IDENTITY_ENV_VAR, KubernetesEventsConfig, KubernetesEventsLeaderElectionConfig,
};

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    pin::Pin,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use futures::{Stream, StreamExt, stream::SelectAll};
use http_1::{HeaderName, HeaderValue};
use k8s_openapi::api::{
    coordination::v1::{Lease, LeaseSpec},
    events::v1::Event as KubeEvent,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{MicroTime, ObjectMeta};
use k8s_openapi::jiff::Timestamp as KubeTimestamp;
use kube::{
    Api, Client, Config as ClientConfig, Error as KubeError,
    api::PostParams,
    config::{KubeConfigOptions, Kubeconfig},
    runtime::{WatchStreamExt, watcher},
};
use tokio::select;
use tokio::time::{Interval, MissedTickBehavior, interval, sleep};
use vector_lib::{
    config::{LegacyKey, LogNamespace, log_schema},
    internal_event::{ComponentEventsDropped, INTENTIONAL},
    lookup::{event_path, path},
};

use crate::{
    SourceSender,
    config::{DataType, SourceConfig, SourceContext, SourceOutput},
    event::{EstimatedJsonEncodedSizeOf, Event, LogEvent},
    internal_events::{
        KubernetesEventsLeaderAcquired, KubernetesEventsLeaderElectionError,
        KubernetesEventsLeaderLost, KubernetesEventsReceived, KubernetesEventsSerializationError,
        KubernetesEventsWatchError, StreamClosedError,
    },
    shutdown::ShutdownSignal,
};
type WatchItem = (Option<String>, watcher::Result<watcher::Event<KubeEvent>>);
type WatchStream = Pin<Box<dyn Stream<Item = WatchItem> + Send>>;

struct EventIdentity {
    uid: String,
    resource_version: String,
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
    leader_election: Option<LeaderElectionSettings>,
}

#[async_trait::async_trait]
#[typetag::serde(name = "kubernetes_events")]
impl SourceConfig for KubernetesEventsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
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
            let Some(leadership_start) = coordinator.wait_for_leadership(shutdown).await else {
                break;
            };

            emit!(KubernetesEventsLeaderAcquired {
                identity: coordinator.settings.identity.clone(),
                lease_namespace: coordinator.settings.lease_namespace.clone(),
                lease_name: coordinator.settings.lease_name.clone(),
            });

            match self
                .run_leadership_epoch(
                    out,
                    shutdown,
                    log_namespace,
                    deduper,
                    &coordinator,
                    leadership_start.suppress_bootstrap_before,
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
        suppress_bootstrap_before: Option<DateTime<Utc>>,
    ) -> Result<LeadershipEnd, ()> {
        let mut streams = self.build_streams();
        let mut renew_interval = interval(coordinator.settings.retry_period);
        renew_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut last_renewal = Instant::now();
        let mut suppress_bootstrap_before = self.bootstrap_cutoffs(suppress_bootstrap_before);

        loop {
            select! {
                _ = &mut *shutdown => return Ok(LeadershipEnd::Shutdown),
                _ = renew_interval.tick() => {
                    if let Some(end) = renew_leadership(coordinator, &mut last_renewal).await {
                        return Ok(end);
                    }
                }
                maybe_event = streams.next() => {
                    match maybe_event {
                        Some((namespace, Ok(event))) => {
                            let init_done = matches!(&event, watcher::Event::InitDone);
                            let bootstrap_cutoff = suppress_bootstrap_before.get(&namespace).copied();
                            if let Some(processed) =
                                self.handle_event(
                                    namespace.as_deref(),
                                    event,
                                    log_namespace,
                                    deduper,
                                    bootstrap_cutoff,
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
                                )
                                .await?
                                {
                                    return Ok(end);
                                }
                                deduper.commit(dedupe_record);
                            }
                            if init_done {
                                suppress_bootstrap_before.remove(&namespace);
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

    fn bootstrap_cutoffs(
        &self,
        cutoff: Option<DateTime<Utc>>,
    ) -> HashMap<Option<String>, DateTime<Utc>> {
        let Some(cutoff) = cutoff else {
            return HashMap::new();
        };

        if self.namespaces.is_empty() {
            return HashMap::from([(None, cutoff)]);
        }

        self.namespaces
            .iter()
            .map(|namespace| (Some(namespace.clone()), cutoff))
            .collect()
    }

    fn handle_event(
        &mut self,
        namespace: Option<&str>,
        event: watcher::Event<KubeEvent>,
        log_namespace: LogNamespace,
        deduper: &mut Deduper,
        suppress_bootstrap_before: Option<DateTime<Utc>>,
    ) -> Result<Option<ProcessedEvent>, ()> {
        match event {
            watcher::Event::Apply(ev) => {
                self.process_apply_event(namespace, ev, log_namespace, deduper)
            }
            watcher::Event::InitApply(ev) => {
                if let Some(cutoff) = suppress_bootstrap_before
                    && self.seed_preexisting_init_apply(&ev, cutoff, deduper)
                {
                    return Ok(None);
                }

                self.process_apply_event(namespace, ev, log_namespace, deduper)
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

        let timestamp = event_timestamp(&event);
        if self.is_older_than(timestamp) {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "expired"
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
        }))
    }

    fn seed_preexisting_init_apply(
        &self,
        event: &KubeEvent,
        cutoff: DateTime<Utc>,
        deduper: &mut Deduper,
    ) -> bool {
        let Some(identity) = event_identity(event) else {
            return true;
        };

        if !self.type_allowed(event) || !self.reason_allowed(event) || !self.kind_allowed(event) {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "filtered"
            });
            return true;
        }

        let timestamp = event_timestamp(event);
        if self.is_older_than(timestamp) {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "expired"
            });
            return true;
        }

        if timestamp >= cutoff || deduper.contains(&identity.uid) {
            return false;
        }

        deduper.prune();
        deduper.commit(PendingDedupeRecord {
            uid: identity.uid,
            resource_version: identity.resource_version,
            event: event.clone(),
        });
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: "initial_replay"
        });

        true
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

#[derive(Clone, Debug)]
struct LeaderElectionSettings {
    lease_name: String,
    lease_namespace: String,
    identity: String,
    lease_duration: Duration,
    renew_deadline: Duration,
    retry_period: Duration,
}

impl LeaderElectionSettings {
    fn from_config(config: &KubernetesEventsLeaderElectionConfig) -> crate::Result<Option<Self>> {
        if !config.enabled {
            return Ok(None);
        }

        if config.lease_duration_seconds == 0 {
            return Err("leader_election.lease_duration_seconds must be greater than 0".into());
        }
        if config.renew_deadline_seconds == 0 {
            return Err("leader_election.renew_deadline_seconds must be greater than 0".into());
        }
        if config.retry_period_seconds == 0 {
            return Err("leader_election.retry_period_seconds must be greater than 0".into());
        }
        if config.renew_deadline_seconds >= config.lease_duration_seconds {
            return Err(
                "leader_election.renew_deadline_seconds must be less than lease_duration_seconds"
                    .into(),
            );
        }
        if config.retry_period_seconds > config.renew_deadline_seconds {
            return Err(
                "leader_election.retry_period_seconds must be less than or equal to renew_deadline_seconds"
                    .into(),
            );
        }

        Ok(Some(Self {
            lease_name: config.lease_name.clone(),
            lease_namespace: resolve_lease_namespace(config.lease_namespace.as_deref()),
            identity: resolve_identity(&config.identity_env_var)?,
            lease_duration: Duration::from_secs(config.lease_duration_seconds),
            renew_deadline: Duration::from_secs(config.renew_deadline_seconds),
            retry_period: Duration::from_secs(config.retry_period_seconds),
        }))
    }
}

struct LeaseCoordinator {
    api: Api<Lease>,
    settings: LeaderElectionSettings,
}

impl LeaseCoordinator {
    fn new(client: Client, settings: LeaderElectionSettings) -> Self {
        let api = Api::namespaced(client, &settings.lease_namespace);
        Self { api, settings }
    }

    async fn wait_for_leadership(&self, shutdown: &mut ShutdownSignal) -> Option<LeadershipStart> {
        loop {
            match self.try_acquire_or_renew().await {
                Ok(LeaseUpdate::Held(start)) => return Some(start),
                Ok(LeaseUpdate::HeldByOther) => {}
                Err(error) => emit!(KubernetesEventsLeaderElectionError { error }),
            }

            select! {
                _ = &mut *shutdown => return None,
                _ = sleep(self.settings.retry_period) => {}
            }
        }
    }

    async fn try_acquire_or_renew(&self) -> Result<LeaseUpdate, KubeError> {
        let now = Utc::now();
        match self.api.get(&self.settings.lease_name).await {
            Ok(lease) => self.update_existing_lease(lease, now).await,
            Err(KubeError::Api(status)) if status.is_not_found() => {
                match self.create_lease(now).await {
                    Ok(_) => Ok(LeaseUpdate::Held(LeadershipStart::default())),
                    Err(KubeError::Api(status))
                        if status.is_already_exists() || status.is_conflict() =>
                    {
                        Ok(LeaseUpdate::HeldByOther)
                    }
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }

    async fn create_lease(&self, now: DateTime<Utc>) -> Result<Lease, KubeError> {
        let lease = Lease {
            metadata: ObjectMeta {
                name: Some(self.settings.lease_name.clone()),
                namespace: Some(self.settings.lease_namespace.clone()),
                ..ObjectMeta::default()
            },
            spec: Some(LeaseSpec {
                acquire_time: Some(kube_micro_time(now)),
                holder_identity: Some(self.settings.identity.clone()),
                lease_duration_seconds: Some(duration_as_i32(self.settings.lease_duration)),
                lease_transitions: Some(0),
                renew_time: Some(kube_micro_time(now)),
                strategy: None,
                preferred_holder: None,
            }),
        };

        self.api.create(&PostParams::default(), &lease).await
    }

    async fn update_existing_lease(
        &self,
        lease: Lease,
        now: DateTime<Utc>,
    ) -> Result<LeaseUpdate, KubeError> {
        let Some(prepared) = prepare_lease_update(lease, &self.settings, now) else {
            return Ok(LeaseUpdate::HeldByOther);
        };

        match self
            .api
            .replace(
                &self.settings.lease_name,
                &PostParams::default(),
                &prepared.lease,
            )
            .await
        {
            Ok(_) => Ok(LeaseUpdate::Held(LeadershipStart {
                suppress_bootstrap_before: prepared.suppress_bootstrap_before,
            })),
            Err(KubeError::Api(status)) if status.is_conflict() => Ok(LeaseUpdate::HeldByOther),
            Err(error) => Err(error),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum LeaseUpdate {
    Held(LeadershipStart),
    HeldByOther,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct LeadershipStart {
    suppress_bootstrap_before: Option<DateTime<Utc>>,
}

enum LeadershipEnd {
    Shutdown,
    Lost(&'static str),
    RestartWatch,
}

struct ProcessedEvent {
    event: Event,
    dedupe_record: PendingDedupeRecord,
}

struct PendingDedupeRecord {
    uid: String,
    resource_version: String,
    event: KubeEvent,
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
                if let Some(end) = renew_leadership(coordinator, last_renewal).await {
                    return Ok(Some(end));
                }
            }
        }
    }
}

async fn renew_leadership(
    coordinator: &LeaseCoordinator,
    last_renewal: &mut Instant,
) -> Option<LeadershipEnd> {
    match coordinator.try_acquire_or_renew().await {
        Ok(LeaseUpdate::Held(_)) => {
            *last_renewal = Instant::now();
            None
        }
        Ok(LeaseUpdate::HeldByOther) => Some(LeadershipEnd::Lost("lease_taken_by_another_holder")),
        Err(error) => {
            emit!(KubernetesEventsLeaderElectionError { error });
            (last_renewal.elapsed() >= coordinator.settings.renew_deadline)
                .then_some(LeadershipEnd::Lost("renew_deadline_exceeded"))
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

    fn evaluate(
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
                    let previous = include_previous.then(|| Box::new(entry.event.clone()));
                    DedupResult::Updated { previous }
                }
            }
            None => DedupResult::Added,
        }
    }

    fn contains(&mut self, uid: &str) -> bool {
        self.prune();
        self.entries.contains_key(uid)
    }

    fn commit(&mut self, record: PendingDedupeRecord) {
        self.entries.insert(
            record.uid,
            CachedEvent {
                event: record.event,
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
        _timestamp: DateTime<Utc>,
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

fn resolve_identity(identity_env_var: &str) -> crate::Result<String> {
    resolve_identity_from(identity_env_var, |name| env::var(name).ok()).map_err(Into::into)
}

fn resolve_identity_from(
    identity_env_var: &str,
    mut get_env: impl FnMut(&str) -> Option<String>,
) -> Result<String, String> {
    if let Some(identity) = get_env(identity_env_var).and_then(non_empty_trimmed) {
        return Ok(identity);
    }

    if identity_env_var != FALLBACK_IDENTITY_ENV_VAR
        && let Some(identity) = get_env(FALLBACK_IDENTITY_ENV_VAR).and_then(non_empty_trimmed)
    {
        return Ok(identity);
    }

    Err(format!(
        "leader election is enabled but neither {identity_env_var} nor {FALLBACK_IDENTITY_ENV_VAR} is set"
    ))
}

fn resolve_lease_namespace(configured: Option<&str>) -> String {
    resolve_lease_namespace_from(
        configured,
        |name| env::var(name).ok(),
        || fs::read_to_string(config::SERVICE_ACCOUNT_NAMESPACE_PATH).ok(),
    )
}

fn resolve_lease_namespace_from(
    configured: Option<&str>,
    mut get_env: impl FnMut(&str) -> Option<String>,
    read_service_account_namespace: impl FnOnce() -> Option<String>,
) -> String {
    configured
        .and_then(non_empty_trimmed)
        .or_else(|| get_env(config::POD_NAMESPACE_ENV_VAR).and_then(non_empty_trimmed))
        .or_else(|| read_service_account_namespace().and_then(non_empty_trimmed))
        .unwrap_or_else(|| "default".to_string())
}

fn non_empty_trimmed(value: impl AsRef<str>) -> Option<String> {
    let value = value.as_ref().trim();
    (!value.is_empty()).then(|| value.to_string())
}

struct PreparedLeaseUpdate {
    lease: Lease,
    suppress_bootstrap_before: Option<DateTime<Utc>>,
}

fn prepare_lease_update(
    mut lease: Lease,
    settings: &LeaderElectionSettings,
    now: DateTime<Utc>,
) -> Option<PreparedLeaseUpdate> {
    let spec = lease.spec.get_or_insert_with(LeaseSpec::default);
    let held_by_self = spec
        .holder_identity
        .as_deref()
        .is_some_and(|holder| holder == settings.identity);

    if !held_by_self && !lease_is_expired(spec, now, settings.lease_duration) {
        return None;
    }

    if !held_by_self {
        spec.acquire_time = Some(kube_micro_time(now));
        spec.lease_transitions = Some(spec.lease_transitions.unwrap_or(0) + 1);
    }

    let previous_holder = spec.holder_identity.clone();
    let previous_renewed_at = spec
        .renew_time
        .as_ref()
        .and_then(|renew_time| kube_timestamp_to_chrono(renew_time.0));
    // After taking over an expired lease, only suppress events older than the previous
    // holder's last renewal. Later events may have been created while no leader was running.
    let suppress_bootstrap_before = (!held_by_self && previous_holder.is_some())
        .then_some(previous_renewed_at)
        .flatten();

    spec.holder_identity = Some(settings.identity.clone());
    spec.lease_duration_seconds = Some(duration_as_i32(settings.lease_duration));
    spec.renew_time = Some(kube_micro_time(now));
    Some(PreparedLeaseUpdate {
        lease,
        suppress_bootstrap_before,
    })
}

fn lease_is_expired(spec: &LeaseSpec, now: DateTime<Utc>, fallback_duration: Duration) -> bool {
    let lease_duration = spec
        .lease_duration_seconds
        .and_then(|duration| u64::try_from(duration).ok())
        .filter(|duration| *duration > 0)
        .map(Duration::from_secs)
        .unwrap_or(fallback_duration);

    let Some(renew_time) = spec.renew_time.as_ref() else {
        return true;
    };
    let Some(renewed_at) = kube_timestamp_to_chrono(renew_time.0) else {
        return true;
    };

    match now.signed_duration_since(renewed_at).to_std() {
        Ok(elapsed) => elapsed > lease_duration,
        Err(_) => false,
    }
}

fn duration_as_i32(duration: Duration) -> i32 {
    i32::try_from(duration.as_secs()).unwrap_or(i32::MAX)
}

fn kube_micro_time(timestamp: DateTime<Utc>) -> MicroTime {
    MicroTime(
        KubeTimestamp::from_microsecond(timestamp.timestamp_micros())
            .expect("timestamp should fit in Kubernetes timestamp range"),
    )
}

fn event_timestamp(event: &KubeEvent) -> DateTime<Utc> {
    event
        .series
        .as_ref()
        .map(|series| series.last_observed_time.0)
        .or_else(|| event.deprecated_last_timestamp.as_ref().map(|t| t.0))
        .or_else(|| event.event_time.as_ref().map(|t| t.0))
        .or_else(|| event.deprecated_first_timestamp.as_ref().map(|t| t.0))
        .or_else(|| event.metadata.creation_timestamp.as_ref().map(|t| t.0))
        .and_then(kube_timestamp_to_chrono)
        .unwrap_or_else(Utc::now)
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

fn kube_timestamp_to_chrono(timestamp: KubeTimestamp) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_micros(timestamp.as_microsecond())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, TimeZone};
    use k8s_openapi::api::events::v1::EventSeries;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{MicroTime, ObjectMeta, Time};
    use vrl::value;

    fn kube_timestamp(timestamp: DateTime<Utc>) -> KubeTimestamp {
        KubeTimestamp::from_microsecond(timestamp.timestamp_micros())
            .expect("timestamp should fit in Kubernetes timestamp range")
    }

    fn make_event(uid: &str, resource_version: &str, timestamp: DateTime<Utc>) -> KubeEvent {
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

    fn make_source() -> KubernetesEventsSource {
        make_source_with_config(KubernetesEventsConfig::default())
    }

    fn make_source_with_config(config: KubernetesEventsConfig) -> KubernetesEventsSource {
        let client_config = ClientConfig::new("http://127.0.0.1:8080".parse().unwrap());
        let client = Client::try_from(client_config).unwrap();
        KubernetesEventsSource::new(client, config).unwrap()
    }

    fn leader_settings(identity: &str) -> LeaderElectionSettings {
        LeaderElectionSettings {
            lease_name: "events".to_string(),
            lease_namespace: "default".to_string(),
            identity: identity.to_string(),
            lease_duration: Duration::from_secs(15),
            renew_deadline: Duration::from_secs(10),
            retry_period: Duration::from_secs(2),
        }
    }

    fn make_lease(
        holder: Option<&str>,
        renew_time: Option<DateTime<Utc>>,
        transitions: Option<i32>,
    ) -> Lease {
        Lease {
            metadata: ObjectMeta {
                name: Some("events".to_string()),
                namespace: Some("default".to_string()),
                resource_version: Some("1".to_string()),
                ..ObjectMeta::default()
            },
            spec: Some(LeaseSpec {
                holder_identity: holder.map(ToString::to_string),
                lease_duration_seconds: Some(15),
                renew_time: renew_time.map(kube_micro_time),
                lease_transitions: transitions,
                ..LeaseSpec::default()
            }),
        }
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
                .metadata
                .resource_version
                .as_deref()),
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
                .metadata
                .resource_version
                .as_deref()),
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

    #[tokio::test]
    async fn leader_bootstrap_seeds_preexisting_init_apply_without_emitting() {
        let mut source = make_source();
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let previous_renewed_at = Utc::now();
        let acquired_at = previous_renewed_at + ChronoDuration::seconds(8);
        let old_event = make_event(
            "old-uid",
            "rv-old",
            previous_renewed_at - ChronoDuration::seconds(10),
        );

        let processed = source
            .handle_event(
                None,
                watcher::Event::InitApply(old_event),
                LogNamespace::Legacy,
                &mut deduper,
                Some(previous_renewed_at),
            )
            .unwrap();

        assert!(
            processed.is_none(),
            "preexisting bootstrap events should seed dedupe without being emitted"
        );
        assert_eq!(
            deduper.entries.get("old-uid").and_then(|entry| entry
                .event
                .metadata
                .resource_version
                .as_deref()),
            Some("rv-old")
        );

        let gap_event = make_event(
            "gap-uid",
            "rv-gap",
            acquired_at - ChronoDuration::seconds(1),
        );
        let processed = source
            .handle_event(
                None,
                watcher::Event::InitApply(gap_event),
                LogNamespace::Legacy,
                &mut deduper,
                Some(previous_renewed_at),
            )
            .unwrap();

        assert!(
            processed.is_some(),
            "events created after the previous renewal should be preserved during failover"
        );
    }

    #[tokio::test]
    async fn leader_bootstrap_suppression_is_tracked_per_namespace() {
        let mut config = KubernetesEventsConfig::default();
        config.namespaces = vec!["first".to_string(), "second".to_string()];
        let mut source = make_source_with_config(config);
        let mut deduper = Deduper::new(Duration::from_secs(60));
        let previous_renewed_at = Utc::now();
        let mut cutoffs = source.bootstrap_cutoffs(Some(previous_renewed_at));

        let first_event = make_event(
            "first-uid",
            "rv-first",
            previous_renewed_at - ChronoDuration::seconds(10),
        );
        let processed = source
            .handle_event(
                Some("first"),
                watcher::Event::InitApply(first_event),
                LogNamespace::Legacy,
                &mut deduper,
                cutoffs.get(&Some("first".to_string())).copied(),
            )
            .unwrap();
        assert!(processed.is_none());
        cutoffs.remove(&Some("first".to_string()));

        let second_event = make_event(
            "second-uid",
            "rv-second",
            previous_renewed_at - ChronoDuration::seconds(10),
        );
        let processed = source
            .handle_event(
                Some("second"),
                watcher::Event::InitApply(second_event),
                LogNamespace::Legacy,
                &mut deduper,
                cutoffs.get(&Some("second".to_string())).copied(),
            )
            .unwrap();

        assert!(
            processed.is_none(),
            "one namespace completing bootstrap must not disable suppression for others"
        );
        assert!(deduper.entries.contains_key("first-uid"));
        assert!(deduper.entries.contains_key("second-uid"));
    }

    #[tokio::test]
    async fn leader_bootstrap_without_cutoff_emits_init_apply() {
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
            "initial bootstrap without a previous holder should emit current events"
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
    fn leader_election_identity_uses_configured_env_var() {
        let identity = resolve_identity_from("POD_NAME", |name| match name {
            "POD_NAME" => Some("vector-0".to_string()),
            FALLBACK_IDENTITY_ENV_VAR => Some("fallback".to_string()),
            _ => None,
        })
        .expect("identity should resolve");

        assert_eq!(identity, "vector-0");
    }

    #[test]
    fn leader_election_identity_falls_back_to_hostname() {
        let identity = resolve_identity_from("POD_NAME", |name| match name {
            FALLBACK_IDENTITY_ENV_VAR => Some("vector-hostname".to_string()),
            _ => None,
        })
        .expect("identity should resolve");

        assert_eq!(identity, "vector-hostname");
    }

    #[test]
    fn leader_election_identity_errors_when_missing() {
        let error =
            resolve_identity_from("POD_NAME", |_| None).expect_err("identity should be required");

        assert!(error.contains("POD_NAME"));
        assert!(error.contains(FALLBACK_IDENTITY_ENV_VAR));
    }

    #[test]
    fn leader_election_namespace_prefers_config() {
        let namespace = resolve_lease_namespace_from(
            Some(" configured "),
            |_| Some("env".to_string()),
            || Some("service-account".to_string()),
        );

        assert_eq!(namespace, "configured");
    }

    #[test]
    fn leader_election_namespace_falls_back_to_service_account() {
        let namespace = resolve_lease_namespace_from(
            None,
            |_| None,
            || Some(" service-account \n".to_string()),
        );

        assert_eq!(namespace, "service-account");
    }

    #[test]
    fn leader_election_namespace_defaults_when_missing() {
        let namespace = resolve_lease_namespace_from(None, |_| None, || None);

        assert_eq!(namespace, "default");
    }

    #[test]
    fn leader_election_renews_lease_held_by_self() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let lease = make_lease(
            Some("vector-0"),
            Some(now - ChronoDuration::seconds(5)),
            Some(2),
        );
        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now)
            .expect("self-held lease should renew");
        let spec = prepared.lease.spec.expect("lease spec should be set");

        assert_eq!(spec.holder_identity.as_deref(), Some("vector-0"));
        assert_eq!(spec.lease_transitions, Some(2));
        assert_eq!(
            spec.renew_time
                .and_then(|time| kube_timestamp_to_chrono(time.0)),
            Some(now)
        );
        assert_eq!(prepared.suppress_bootstrap_before, None);
    }

    #[test]
    fn leader_election_does_not_take_unexpired_lease_held_by_other() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let lease = make_lease(
            Some("vector-1"),
            Some(now - ChronoDuration::seconds(5)),
            Some(2),
        );

        assert!(prepare_lease_update(lease, &leader_settings("vector-0"), now).is_none());
    }

    #[test]
    fn leader_election_takes_expired_lease_held_by_other() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let lease = make_lease(
            Some("vector-1"),
            Some(now - ChronoDuration::seconds(16)),
            Some(2),
        );
        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now)
            .expect("expired lease should be acquired");
        let spec = prepared.lease.spec.expect("lease spec should be set");

        assert_eq!(spec.holder_identity.as_deref(), Some("vector-0"));
        assert_eq!(spec.lease_transitions, Some(3));
        assert_eq!(
            spec.acquire_time
                .and_then(|time| kube_timestamp_to_chrono(time.0)),
            Some(now)
        );
        assert_eq!(
            prepared.suppress_bootstrap_before,
            Some(now - ChronoDuration::seconds(16))
        );
    }

    #[test]
    fn leader_election_takes_lease_without_holder() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let lease = make_lease(None, None, None);
        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now)
            .expect("empty lease should be acquired");
        let spec = prepared.lease.spec.expect("lease spec should be set");

        assert_eq!(spec.holder_identity.as_deref(), Some("vector-0"));
        assert_eq!(spec.lease_transitions, Some(1));
        assert_eq!(prepared.suppress_bootstrap_before, None);
    }
}
