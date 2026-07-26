//! Lease-based leader election and resource-version checkpoint persistence.
//!
//! Replicas coordinate through a `coordination.k8s.io/v1` Lease. The active leader additionally
//! records the last safely handled Kubernetes `resourceVersion` for each watch stream as an
//! annotation on the Lease. A replica taking over can therefore resume each watch from API-server
//! progress rather than guessing from event timestamps.

use std::{
    collections::BTreeMap,
    env, fs,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use k8s_openapi::api::coordination::v1::{Lease, LeaseSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{MicroTime, ObjectMeta};
use k8s_openapi::jiff::Timestamp as KubeTimestamp;
use kube::{Api, Client, Error as KubeError, api::PostParams};
use serde::{Deserialize, Serialize};
use tokio::select;
use tokio::time::sleep;

use super::config::{self, FALLBACK_IDENTITY_ENV_VAR, KubernetesEventsLeaderElectionConfig};
use super::kube_timestamp_to_chrono;
use crate::{internal_events::KubernetesEventsLeaderElectionError, shutdown::ShutdownSignal};

const CHECKPOINTS_ANNOTATION: &str = "kubernetes-events.vector.dev/resource-version-checkpoints";
const LEGACY_WATERMARK_ANNOTATION: &str = "kubernetes-events.vector.dev/watermark";
const CHECKPOINTS_VERSION: u8 = 1;

/// Resource versions that have been safely handled, keyed by logical watch stream.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct ResourceVersionCheckpoints {
    version: u8,
    configuration: String,
    streams: BTreeMap<String, String>,
}

impl ResourceVersionCheckpoints {
    pub(super) const fn new(configuration: String) -> Self {
        Self {
            version: CHECKPOINTS_VERSION,
            configuration,
            streams: BTreeMap::new(),
        }
    }

    fn resume(persisted: Option<Self>, configuration: &str) -> Self {
        match persisted {
            Some(checkpoints)
                if checkpoints.version == CHECKPOINTS_VERSION
                    && checkpoints.configuration == configuration =>
            {
                checkpoints
            }
            _ => Self::new(configuration.to_string()),
        }
    }

    pub(super) fn get(&self, stream: &str) -> Option<&str> {
        self.streams.get(stream).map(String::as_str)
    }

    pub(super) fn set(&mut self, stream: String, resource_version: String) {
        if !resource_version.is_empty() {
            self.streams.insert(stream, resource_version);
        }
    }

    pub(super) fn len(&self) -> usize {
        self.streams.len()
    }
}

#[derive(Clone, Debug)]
pub(super) struct LeaderElectionSettings {
    pub(super) lease_name: String,
    pub(super) lease_namespace: String,
    pub(super) identity: String,
    pub(super) lease_duration: Duration,
    pub(super) renew_deadline: Duration,
    pub(super) retry_period: Duration,
}

impl LeaderElectionSettings {
    pub(super) fn from_config(
        config: &KubernetesEventsLeaderElectionConfig,
    ) -> crate::Result<Option<Self>> {
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

pub(super) struct LeaseCoordinator {
    api: Api<Lease>,
    pub(super) settings: LeaderElectionSettings,
}

impl LeaseCoordinator {
    pub(super) fn new(client: Client, settings: LeaderElectionSettings) -> Self {
        let api = Api::namespaced(client, &settings.lease_namespace);
        Self { api, settings }
    }

    pub(super) async fn wait_for_leadership(
        &self,
        shutdown: &mut ShutdownSignal,
        checkpoint_configuration: &str,
    ) -> Option<AcquiredLeadership> {
        loop {
            match self.try_acquire_or_renew(None).await {
                Ok(LeaseUpdate::Held { prior_checkpoints }) => {
                    return Some(AcquiredLeadership {
                        checkpoints: ResourceVersionCheckpoints::resume(
                            prior_checkpoints,
                            checkpoint_configuration,
                        ),
                    });
                }
                Ok(LeaseUpdate::HeldByOther) => {}
                Err(error) => emit!(KubernetesEventsLeaderElectionError { error }),
            }

            select! {
                _ = &mut *shutdown => return None,
                _ = sleep(self.settings.retry_period) => {}
            }
        }
    }

    async fn try_acquire_or_renew(
        &self,
        checkpoints: Option<&ResourceVersionCheckpoints>,
    ) -> Result<LeaseUpdate, KubeError> {
        let now = Utc::now();
        match self.api.get(&self.settings.lease_name).await {
            Ok(lease) => self.update_existing_lease(lease, now, checkpoints).await,
            Err(KubeError::Api(status)) if status.is_not_found() => {
                match self.create_lease(now, checkpoints).await {
                    Ok(_) => Ok(LeaseUpdate::Held {
                        prior_checkpoints: None,
                    }),
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

    async fn create_lease(
        &self,
        now: DateTime<Utc>,
        checkpoints: Option<&ResourceVersionCheckpoints>,
    ) -> Result<Lease, KubeError> {
        let lease = Lease {
            metadata: ObjectMeta {
                name: Some(self.settings.lease_name.clone()),
                namespace: Some(self.settings.lease_namespace.clone()),
                annotations: checkpoints.map(|checkpoints| {
                    [(
                        CHECKPOINTS_ANNOTATION.to_string(),
                        serialize_checkpoints(checkpoints),
                    )]
                    .into_iter()
                    .collect()
                }),
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
        checkpoints: Option<&ResourceVersionCheckpoints>,
    ) -> Result<LeaseUpdate, KubeError> {
        let prior_checkpoints = lease_checkpoints(&lease);
        let Some(updated) = prepare_lease_update(lease, &self.settings, now, checkpoints) else {
            return Ok(LeaseUpdate::HeldByOther);
        };

        match self
            .api
            .replace(&self.settings.lease_name, &PostParams::default(), &updated)
            .await
        {
            Ok(_) => Ok(LeaseUpdate::Held { prior_checkpoints }),
            Err(KubeError::Api(status)) if status.is_conflict() => Ok(LeaseUpdate::HeldByOther),
            Err(error) => Err(error),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum LeaseUpdate {
    Held {
        prior_checkpoints: Option<ResourceVersionCheckpoints>,
    },
    HeldByOther,
}

/// The state observed on the Lease at the moment leadership was acquired.
pub(super) struct AcquiredLeadership {
    pub(super) checkpoints: ResourceVersionCheckpoints,
}

pub(super) enum LeadershipEnd {
    Shutdown,
    Lost(&'static str),
    RestartWatch,
}

pub(super) async fn renew_leadership(
    coordinator: &LeaseCoordinator,
    last_renewal: &mut Instant,
    checkpoints: &ResourceVersionCheckpoints,
) -> Option<LeadershipEnd> {
    match coordinator.try_acquire_or_renew(Some(checkpoints)).await {
        Ok(LeaseUpdate::Held { .. }) => {
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

fn prepare_lease_update(
    mut lease: Lease,
    settings: &LeaderElectionSettings,
    now: DateTime<Utc>,
    checkpoints: Option<&ResourceVersionCheckpoints>,
) -> Option<Lease> {
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

    spec.holder_identity = Some(settings.identity.clone());
    spec.lease_duration_seconds = Some(duration_as_i32(settings.lease_duration));
    spec.renew_time = Some(kube_micro_time(now));

    if let Some(checkpoints) = checkpoints {
        let annotations = lease
            .metadata
            .annotations
            .get_or_insert_with(Default::default);
        annotations.remove(LEGACY_WATERMARK_ANNOTATION);
        annotations.insert(
            CHECKPOINTS_ANNOTATION.to_string(),
            serialize_checkpoints(checkpoints),
        );
    }

    Some(lease)
}

fn lease_checkpoints(lease: &Lease) -> Option<ResourceVersionCheckpoints> {
    lease
        .metadata
        .annotations
        .as_ref()?
        .get(CHECKPOINTS_ANNOTATION)
        .and_then(|raw| serde_json::from_str(raw).ok())
}

fn serialize_checkpoints(checkpoints: &ResourceVersionCheckpoints) -> String {
    serde_json::to_string(checkpoints)
        .expect("resource-version checkpoints contain only serializable values")
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, TimeZone};

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

    fn checkpoints(configuration: &str, entries: &[(&str, &str)]) -> ResourceVersionCheckpoints {
        let mut checkpoints = ResourceVersionCheckpoints::new(configuration.to_string());
        for (stream, resource_version) in entries {
            checkpoints.set((*stream).to_string(), (*resource_version).to_string());
        }
        checkpoints
    }

    fn annotate_checkpoints(lease: &mut Lease, checkpoints: &ResourceVersionCheckpoints) {
        lease
            .metadata
            .annotations
            .get_or_insert_with(Default::default)
            .insert(
                CHECKPOINTS_ANNOTATION.to_string(),
                serialize_checkpoints(checkpoints),
            );
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
        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now, None)
            .expect("self-held lease should renew");
        let spec = prepared.spec.expect("lease spec should be set");

        assert_eq!(spec.holder_identity.as_deref(), Some("vector-0"));
        assert_eq!(spec.lease_transitions, Some(2));
        assert_eq!(
            spec.renew_time
                .and_then(|time| kube_timestamp_to_chrono(time.0)),
            Some(now)
        );
    }

    #[test]
    fn leader_election_does_not_take_unexpired_lease_held_by_other() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let lease = make_lease(
            Some("vector-1"),
            Some(now - ChronoDuration::seconds(5)),
            Some(2),
        );

        assert!(prepare_lease_update(lease, &leader_settings("vector-0"), now, None).is_none());
    }

    #[test]
    fn leader_election_takes_expired_lease_held_by_other() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let lease = make_lease(
            Some("vector-1"),
            Some(now - ChronoDuration::seconds(16)),
            Some(2),
        );
        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now, None)
            .expect("expired lease should be acquired");
        let spec = prepared.spec.expect("lease spec should be set");

        assert_eq!(spec.holder_identity.as_deref(), Some("vector-0"));
        assert_eq!(spec.lease_transitions, Some(3));
        assert_eq!(
            spec.acquire_time
                .and_then(|time| kube_timestamp_to_chrono(time.0)),
            Some(now)
        );
    }

    #[test]
    fn leader_election_takes_lease_without_holder() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let lease = make_lease(None, None, None);
        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now, None)
            .expect("empty lease should be acquired");
        let spec = prepared.spec.expect("lease spec should be set");

        assert_eq!(spec.holder_identity.as_deref(), Some("vector-0"));
        assert_eq!(spec.lease_transitions, Some(1));
    }

    #[test]
    fn lease_checkpoints_round_trip_through_annotation() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let expected = checkpoints("config", &[("all", "123"), ("namespace/default", "120")]);
        let mut lease = make_lease(Some("vector-0"), Some(now), Some(1));
        annotate_checkpoints(&mut lease, &expected);

        assert_eq!(lease_checkpoints(&lease), Some(expected));
    }

    #[test]
    fn lease_checkpoints_ignore_missing_or_invalid_annotation() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let lease = make_lease(Some("vector-0"), Some(now), Some(1));
        assert_eq!(lease_checkpoints(&lease), None);

        let mut lease = make_lease(Some("vector-0"), Some(now), Some(1));
        lease
            .metadata
            .annotations
            .get_or_insert_with(Default::default)
            .insert(CHECKPOINTS_ANNOTATION.to_string(), "not-json".into());
        assert_eq!(lease_checkpoints(&lease), None);
    }

    #[test]
    fn checkpoint_resume_requires_matching_configuration_and_version() {
        let persisted = checkpoints("config-a", &[("all", "123")]);
        assert_eq!(
            ResourceVersionCheckpoints::resume(Some(persisted.clone()), "config-a"),
            persisted
        );

        let reset = ResourceVersionCheckpoints::resume(Some(persisted.clone()), "config-b");
        assert_eq!(reset.get("all"), None);

        let mut future = persisted;
        future.version = CHECKPOINTS_VERSION + 1;
        assert_eq!(
            ResourceVersionCheckpoints::resume(Some(future), "config-a").get("all"),
            None
        );
    }

    #[test]
    fn lease_update_preserves_checkpoints_on_acquisition() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let expected = checkpoints("config", &[("all", "123")]);
        let mut lease = make_lease(
            Some("vector-1"),
            Some(now - ChronoDuration::seconds(16)),
            Some(2),
        );
        annotate_checkpoints(&mut lease, &expected);

        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now, None)
            .expect("expired lease should be acquired");

        assert_eq!(
            lease_checkpoints(&prepared),
            Some(expected),
            "acquiring a lease must not discard the previous holder's checkpoints"
        );
    }

    #[test]
    fn lease_update_replaces_checkpoints_on_renewal() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let old = checkpoints("config", &[("all", "100")]);
        let updated = checkpoints("config", &[("all", "123")]);
        let mut lease = make_lease(
            Some("vector-0"),
            Some(now - ChronoDuration::seconds(2)),
            Some(1),
        );
        annotate_checkpoints(&mut lease, &old);

        let prepared =
            prepare_lease_update(lease, &leader_settings("vector-0"), now, Some(&updated))
                .expect("self-held lease should renew");

        assert_eq!(lease_checkpoints(&prepared), Some(updated));
    }

    #[test]
    fn lease_update_keeps_unrelated_annotations() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let mut lease = make_lease(
            Some("vector-0"),
            Some(now - ChronoDuration::seconds(2)),
            Some(1),
        );
        lease
            .metadata
            .annotations
            .get_or_insert_with(Default::default)
            .insert("example.com/other".to_string(), "keep-me".to_string());
        let updated = checkpoints("config", &[("all", "123")]);

        let prepared =
            prepare_lease_update(lease, &leader_settings("vector-0"), now, Some(&updated))
                .expect("self-held lease should renew");

        assert_eq!(
            prepared
                .metadata
                .annotations
                .as_ref()
                .and_then(|annotations| annotations.get("example.com/other"))
                .map(String::as_str),
            Some("keep-me")
        );
    }

    #[test]
    fn checkpoint_write_removes_legacy_watermark() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let mut lease = make_lease(Some("vector-0"), Some(now), Some(1));
        lease
            .metadata
            .annotations
            .get_or_insert_with(Default::default)
            .insert(
                LEGACY_WATERMARK_ANNOTATION.to_string(),
                "legacy".to_string(),
            );
        let updated = checkpoints("config", &[("all", "123")]);

        let prepared =
            prepare_lease_update(lease, &leader_settings("vector-0"), now, Some(&updated))
                .expect("self-held lease should renew");

        assert!(
            prepared
                .metadata
                .annotations
                .as_ref()
                .is_none_or(|annotations| !annotations.contains_key(LEGACY_WATERMARK_ANNOTATION))
        );
    }
}
