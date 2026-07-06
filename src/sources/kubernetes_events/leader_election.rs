//! Lease-based leader election and delivery-watermark persistence.
//!
//! Replicas coordinate through a `coordination.k8s.io/v1` Lease. The active leader additionally
//! records the newest event timestamp it has forwarded downstream (the delivery watermark) as an
//! annotation on the Lease, piggybacked on the periodic renewal write, so that a replica taking
//! over can resume where the previous leader stopped.

use std::{
    env, fs,
    time::{Duration, Instant},
};

use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use k8s_openapi::api::coordination::v1::{Lease, LeaseSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{MicroTime, ObjectMeta};
use k8s_openapi::jiff::Timestamp as KubeTimestamp;
use kube::{Api, Client, Error as KubeError, api::PostParams};
use tokio::select;
use tokio::time::sleep;

use super::config::{self, FALLBACK_IDENTITY_ENV_VAR, KubernetesEventsLeaderElectionConfig};
use super::kube_timestamp_to_chrono;
use crate::{internal_events::KubernetesEventsLeaderElectionError, shutdown::ShutdownSignal};

/// Lease annotation holding the newest event timestamp the leader has forwarded downstream.
const WATERMARK_ANNOTATION: &str = "kubernetes-events.vector.dev/watermark";

#[derive(Clone, Debug)]
pub(super) struct LeaderElectionSettings {
    pub(super) lease_name: String,
    pub(super) lease_namespace: String,
    pub(super) identity: String,
    pub(super) lease_duration: Duration,
    pub(super) renew_deadline: Duration,
    pub(super) retry_period: Duration,
    pub(super) watermark_grace: Duration,
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
            watermark_grace: Duration::from_secs(config.watermark_grace_seconds),
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
    ) -> Option<AcquiredLeadership> {
        loop {
            match self.try_acquire_or_renew(None).await {
                Ok(LeaseUpdate::Held { prior_watermark }) => {
                    return Some(AcquiredLeadership {
                        watermark: prior_watermark,
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
        watermark: Option<DateTime<Utc>>,
    ) -> Result<LeaseUpdate, KubeError> {
        let now = Utc::now();
        match self.api.get(&self.settings.lease_name).await {
            Ok(lease) => self.update_existing_lease(lease, now, watermark).await,
            Err(KubeError::Api(status)) if status.is_not_found() => {
                match self.create_lease(now, watermark).await {
                    Ok(_) => Ok(LeaseUpdate::Held {
                        prior_watermark: None,
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
        watermark: Option<DateTime<Utc>>,
    ) -> Result<Lease, KubeError> {
        let lease = Lease {
            metadata: ObjectMeta {
                name: Some(self.settings.lease_name.clone()),
                namespace: Some(self.settings.lease_namespace.clone()),
                annotations: watermark.map(|timestamp| {
                    [(
                        WATERMARK_ANNOTATION.to_string(),
                        format_watermark(timestamp),
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
        watermark: Option<DateTime<Utc>>,
    ) -> Result<LeaseUpdate, KubeError> {
        let prior_watermark = lease_watermark(&lease);
        let Some(updated) = prepare_lease_update(lease, &self.settings, now, watermark) else {
            return Ok(LeaseUpdate::HeldByOther);
        };

        match self
            .api
            .replace(&self.settings.lease_name, &PostParams::default(), &updated)
            .await
        {
            Ok(_) => Ok(LeaseUpdate::Held { prior_watermark }),
            Err(KubeError::Api(status)) if status.is_conflict() => Ok(LeaseUpdate::HeldByOther),
            Err(error) => Err(error),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum LeaseUpdate {
    Held {
        prior_watermark: Option<DateTime<Utc>>,
    },
    HeldByOther,
}

/// The state observed on the Lease at the moment leadership was acquired.
pub(super) struct AcquiredLeadership {
    pub(super) watermark: Option<DateTime<Utc>>,
}

pub(super) enum LeadershipEnd {
    Shutdown,
    Lost(&'static str),
    RestartWatch,
}

pub(super) async fn renew_leadership(
    coordinator: &LeaseCoordinator,
    last_renewal: &mut Instant,
    watermark: Option<DateTime<Utc>>,
) -> Option<LeadershipEnd> {
    match coordinator.try_acquire_or_renew(watermark).await {
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
    watermark: Option<DateTime<Utc>>,
) -> Option<Lease> {
    // The stored watermark never regresses, and acquisitions (which pass no watermark of their
    // own yet) preserve the previous holder's value.
    let effective_watermark = max_watermark(lease_watermark(&lease), watermark);

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

    if let Some(timestamp) = effective_watermark {
        lease
            .metadata
            .annotations
            .get_or_insert_with(Default::default)
            .insert(
                WATERMARK_ANNOTATION.to_string(),
                format_watermark(timestamp),
            );
    }

    Some(lease)
}

fn lease_watermark(lease: &Lease) -> Option<DateTime<Utc>> {
    lease
        .metadata
        .annotations
        .as_ref()?
        .get(WATERMARK_ANNOTATION)
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

fn format_watermark(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Micros, true)
}

pub(super) fn max_watermark(
    current: Option<DateTime<Utc>>,
    candidate: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.max(candidate)),
        (current, candidate) => current.or(candidate),
    }
}

/// The timestamp at or below which initial-list replays are dropped for the current leadership
/// epoch. `None` disables replay filtering (no stored watermark, or a grace window that exceeds
/// the representable range).
pub(super) fn replay_cutoff(
    watermark: Option<DateTime<Utc>>,
    grace: Duration,
) -> Option<DateTime<Utc>> {
    let watermark = watermark?;
    TimeDelta::from_std(grace)
        .ok()
        .and_then(|grace| watermark.checked_sub_signed(grace))
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
            watermark_grace: Duration::from_secs(600),
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

    fn annotate_watermark(lease: &mut Lease, timestamp: DateTime<Utc>) {
        lease
            .metadata
            .annotations
            .get_or_insert_with(Default::default)
            .insert(
                WATERMARK_ANNOTATION.to_string(),
                format_watermark(timestamp),
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
    fn lease_watermark_round_trips_through_annotation() {
        let timestamp = Utc.timestamp_micros(1_700_000_000_123_456).unwrap();
        let mut lease = make_lease(Some("vector-0"), Some(timestamp), Some(1));
        annotate_watermark(&mut lease, timestamp);

        assert_eq!(lease_watermark(&lease), Some(timestamp));
    }

    #[test]
    fn lease_watermark_ignores_missing_or_invalid_annotation() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let lease = make_lease(Some("vector-0"), Some(now), Some(1));
        assert_eq!(lease_watermark(&lease), None);

        let mut lease = make_lease(Some("vector-0"), Some(now), Some(1));
        lease
            .metadata
            .annotations
            .get_or_insert_with(Default::default)
            .insert(WATERMARK_ANNOTATION.to_string(), "not-a-timestamp".into());
        assert_eq!(lease_watermark(&lease), None);
    }

    #[test]
    fn lease_update_preserves_watermark_on_acquisition() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let watermark = now - ChronoDuration::seconds(60);
        // Expired lease held by the previous leader, carrying its watermark.
        let mut lease = make_lease(
            Some("vector-1"),
            Some(now - ChronoDuration::seconds(16)),
            Some(2),
        );
        annotate_watermark(&mut lease, watermark);

        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now, None)
            .expect("expired lease should be acquired");

        assert_eq!(
            lease_watermark(&prepared),
            Some(watermark),
            "acquiring a lease must not discard the previous holder's watermark"
        );
    }

    #[test]
    fn lease_update_advances_watermark_on_renewal() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let old_watermark = now - ChronoDuration::seconds(120);
        let new_watermark = now - ChronoDuration::seconds(5);
        let mut lease = make_lease(
            Some("vector-0"),
            Some(now - ChronoDuration::seconds(2)),
            Some(1),
        );
        annotate_watermark(&mut lease, old_watermark);

        let prepared = prepare_lease_update(
            lease,
            &leader_settings("vector-0"),
            now,
            Some(new_watermark),
        )
        .expect("self-held lease should renew");

        assert_eq!(lease_watermark(&prepared), Some(new_watermark));
    }

    #[test]
    fn lease_update_never_regresses_watermark() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let newer = now - ChronoDuration::seconds(5);
        let older = now - ChronoDuration::seconds(120);
        let mut lease = make_lease(
            Some("vector-0"),
            Some(now - ChronoDuration::seconds(2)),
            Some(1),
        );
        annotate_watermark(&mut lease, newer);

        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now, Some(older))
            .expect("self-held lease should renew");

        assert_eq!(
            lease_watermark(&prepared),
            Some(newer),
            "a stale caller-provided watermark must not overwrite a newer stored one"
        );
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

        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now, Some(now))
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
    fn max_watermark_prefers_newest() {
        let older = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let newer = older + ChronoDuration::seconds(10);

        assert_eq!(max_watermark(Some(older), Some(newer)), Some(newer));
        assert_eq!(max_watermark(Some(newer), Some(older)), Some(newer));
        assert_eq!(max_watermark(None, Some(older)), Some(older));
        assert_eq!(max_watermark(Some(older), None), Some(older));
        assert_eq!(max_watermark(None, None), None);
    }

    #[test]
    fn replay_cutoff_subtracts_grace() {
        let watermark = Utc.timestamp_opt(1_700_000_000, 0).unwrap();

        assert_eq!(
            replay_cutoff(Some(watermark), Duration::from_secs(600)),
            Some(watermark - ChronoDuration::seconds(600))
        );
        assert_eq!(
            replay_cutoff(Some(watermark), Duration::ZERO),
            Some(watermark)
        );
        assert_eq!(replay_cutoff(None, Duration::from_secs(600)), None);
    }
}
