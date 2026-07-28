//! Lease-based leader election and resource-version checkpoint persistence.
//!
//! Replicas coordinate through a `coordination.k8s.io/v1` Lease. The active leader additionally
//! records the last safely handled Kubernetes `resourceVersion` for each watch stream as an
//! annotation on the Lease. A replica taking over can therefore resume each watch from API-server
//! progress rather than guessing from event timestamps.

use std::{
    collections::BTreeMap,
    env, fs,
    sync::Mutex,
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
#[cfg(test)]
use super::kube_timestamp_to_chrono;
use crate::{
    internal_events::{KubernetesEventsCheckpointTooLarge, KubernetesEventsLeaderElectionError},
    shutdown::ShutdownSignal,
};

const CHECKPOINTS_ANNOTATION: &str = "kubernetes-events.vector.dev/resource-version-checkpoints";
const LEGACY_WATERMARK_ANNOTATION: &str = "kubernetes-events.vector.dev/watermark";
const CHECKPOINTS_VERSION: u8 = 2;
// Kubernetes validates the combined byte length of all annotation keys and values against this
// limit. Keeping the calculation local avoids turning an oversized checkpoint into renewal loss.
const KUBERNETES_ANNOTATIONS_SIZE_LIMIT_BYTES: usize = 256 * 1024;

/// Resource versions that have been safely handled, keyed by logical watch stream.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct ResourceVersionCheckpoints {
    version: u8,
    configuration_hash: String,
    streams: BTreeMap<String, String>,
}

impl ResourceVersionCheckpoints {
    pub(super) const fn new(configuration_hash: String) -> Self {
        Self {
            version: CHECKPOINTS_VERSION,
            configuration_hash,
            streams: BTreeMap::new(),
        }
    }

    fn resume(persisted: Option<Self>, configuration_hash: &str) -> Self {
        match persisted {
            Some(checkpoints)
                if checkpoints.version == CHECKPOINTS_VERSION
                    && checkpoints.configuration_hash == configuration_hash =>
            {
                checkpoints
            }
            _ => Self::new(configuration_hash.to_string()),
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
        let lease_name = config
            .lease_name
            .as_deref()
            .and_then(non_empty_trimmed)
            .ok_or("leader_election.lease_name must be set when leader election is enabled")?;
        if config.renew_deadline_seconds >= config.lease_duration_seconds {
            return Err(
                "leader_election.renew_deadline_seconds must be less than lease_duration_seconds"
                    .into(),
            );
        }
        if config.retry_period_seconds >= config.renew_deadline_seconds {
            return Err(
                "leader_election.retry_period_seconds must be less than renew_deadline_seconds"
                    .into(),
            );
        }

        Ok(Some(Self {
            lease_name,
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
    observed_lease: Mutex<Option<ObservedLease>>,
}

struct ObservedLease {
    spec: LeaseSpec,
    observed_at: Instant,
}

impl LeaseCoordinator {
    pub(super) fn new(client: Client, settings: LeaderElectionSettings) -> Self {
        let api = Api::namespaced(client, &settings.lease_namespace);
        Self {
            api,
            settings,
            observed_lease: Mutex::new(None),
        }
    }

    pub(super) async fn wait_for_leadership(
        &self,
        shutdown: &mut ShutdownSignal,
        checkpoint_configuration_hash: &str,
    ) -> Option<AcquiredLeadership> {
        loop {
            match self.try_acquire_or_renew(None).await {
                Ok(LeaseUpdate::Held { prior_checkpoints }) => {
                    return Some(AcquiredLeadership {
                        checkpoints: ResourceVersionCheckpoints::resume(
                            prior_checkpoints,
                            checkpoint_configuration_hash,
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
        let mut metadata = ObjectMeta {
            name: Some(self.settings.lease_name.clone()),
            namespace: Some(self.settings.lease_namespace.clone()),
            ..ObjectMeta::default()
        };
        if let Some(checkpoints) = checkpoints {
            set_checkpoint_annotation(&mut metadata, checkpoints);
        }

        let lease = Lease {
            metadata,
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

        let created = self.api.create(&PostParams::default(), &lease).await?;
        self.observe_lease(&created);
        Ok(created)
    }

    async fn update_existing_lease(
        &self,
        lease: Lease,
        now: DateTime<Utc>,
        checkpoints: Option<&ResourceVersionCheckpoints>,
    ) -> Result<LeaseUpdate, KubeError> {
        let prior_checkpoints = lease_checkpoints(&lease);
        let locally_expired = lease
            .spec
            .as_ref()
            .is_some_and(|spec| self.observed_lease_is_expired(spec));
        let Some(updated) =
            prepare_lease_update(lease, &self.settings, now, locally_expired, checkpoints)
        else {
            return Ok(LeaseUpdate::HeldByOther);
        };

        let replaced = self
            .api
            .replace(&self.settings.lease_name, &PostParams::default(), &updated)
            .await?;
        self.observe_lease(&replaced);
        Ok(LeaseUpdate::Held { prior_checkpoints })
    }

    fn observed_lease_is_expired(&self, spec: &LeaseSpec) -> bool {
        let mut observed = self
            .observed_lease
            .lock()
            .expect("lease observation mutex should not be poisoned");
        locally_observed_lease_is_expired(
            &mut observed,
            spec,
            Instant::now(),
            self.settings.lease_duration,
        )
    }

    fn observe_lease(&self, lease: &Lease) {
        if let Some(spec) = lease.spec.clone() {
            *self
                .observed_lease
                .lock()
                .expect("lease observation mutex should not be poisoned") = Some(ObservedLease {
                spec,
                observed_at: Instant::now(),
            });
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
    locally_expired: bool,
    checkpoints: Option<&ResourceVersionCheckpoints>,
) -> Option<Lease> {
    let spec = lease.spec.get_or_insert_with(LeaseSpec::default);
    let held_by_self = spec
        .holder_identity
        .as_deref()
        .is_some_and(|holder| holder == settings.identity);

    let held_by_other = spec
        .holder_identity
        .as_deref()
        .is_some_and(|holder| !holder.is_empty() && holder != settings.identity);

    if held_by_other && !locally_expired {
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
        set_checkpoint_annotation(&mut lease.metadata, checkpoints);
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

fn set_checkpoint_annotation(metadata: &mut ObjectMeta, checkpoints: &ResourceVersionCheckpoints) {
    let serialized = serialize_checkpoints(checkpoints);
    let other_annotations_size = metadata
        .annotations
        .as_ref()
        .into_iter()
        .flatten()
        .filter(|(key, _)| {
            key.as_str() != CHECKPOINTS_ANNOTATION && key.as_str() != LEGACY_WATERMARK_ANNOTATION
        })
        .map(|(key, value)| key.len() + value.len())
        .sum::<usize>();
    let size = other_annotations_size + CHECKPOINTS_ANNOTATION.len() + serialized.len();

    let annotations = metadata.annotations.get_or_insert_with(Default::default);
    annotations.remove(LEGACY_WATERMARK_ANNOTATION);
    if size <= KUBERNETES_ANNOTATIONS_SIZE_LIMIT_BYTES {
        annotations.insert(CHECKPOINTS_ANNOTATION.to_string(), serialized);
    } else {
        annotations.remove(CHECKPOINTS_ANNOTATION);
        emit!(KubernetesEventsCheckpointTooLarge {
            size,
            limit: KUBERNETES_ANNOTATIONS_SIZE_LIMIT_BYTES,
        });
    }

    if annotations.is_empty() {
        metadata.annotations = None;
    }
}

fn locally_observed_lease_is_expired(
    observed: &mut Option<ObservedLease>,
    spec: &LeaseSpec,
    now: Instant,
    fallback_duration: Duration,
) -> bool {
    if observed
        .as_ref()
        .is_none_or(|observed| observed.spec != *spec)
    {
        *observed = Some(ObservedLease {
            spec: spec.clone(),
            observed_at: now,
        });
    }

    let lease_duration = spec
        .lease_duration_seconds
        .and_then(|duration| u64::try_from(duration).ok())
        .filter(|duration| *duration > 0)
        .map(Duration::from_secs)
        .unwrap_or(fallback_duration);

    observed.as_ref().is_some_and(|observed| {
        now.saturating_duration_since(observed.observed_at) >= lease_duration
    })
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
    use http_1::{Request, Response, StatusCode, header::CONTENT_TYPE};
    use kube::client::Body;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tower::service_fn;

    fn json_response(status: StatusCode, value: impl Into<Vec<u8>>) -> Response<Body> {
        Response::builder()
            .status(status)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(value.into()))
            .unwrap()
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
    fn leader_election_requires_an_explicit_lease_name() {
        let config = KubernetesEventsLeaderElectionConfig {
            enabled: true,
            ..KubernetesEventsLeaderElectionConfig::default()
        };

        let error = LeaderElectionSettings::from_config(&config)
            .expect_err("an enabled election must identify its logical source");

        assert_eq!(
            error.to_string(),
            "leader_election.lease_name must be set when leader election is enabled"
        );
    }

    #[test]
    fn leader_election_rejects_retry_period_equal_to_renew_deadline() {
        let config = KubernetesEventsLeaderElectionConfig {
            enabled: true,
            lease_name: Some("events".to_string()),
            renew_deadline_seconds: 10,
            retry_period_seconds: 10,
            ..KubernetesEventsLeaderElectionConfig::default()
        };

        let error = LeaderElectionSettings::from_config(&config)
            .expect_err("the schedule must leave time for a retry before the deadline");

        assert_eq!(
            error.to_string(),
            "leader_election.retry_period_seconds must be less than renew_deadline_seconds"
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
        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now, false, None)
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

        assert!(
            prepare_lease_update(lease, &leader_settings("vector-0"), now, false, None).is_none()
        );
    }

    #[test]
    fn leader_election_takes_expired_lease_held_by_other() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let lease = make_lease(
            Some("vector-1"),
            Some(now - ChronoDuration::seconds(16)),
            Some(2),
        );
        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now, true, None)
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
    fn leader_election_lease_expiry_uses_local_observation_time() {
        let remote_now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let mut spec = make_lease(
            Some("vector-1"),
            Some(remote_now - ChronoDuration::hours(1)),
            Some(2),
        )
        .spec
        .expect("lease spec should be set");
        let mut observed = None;
        let first_observation = Instant::now();
        let lease_duration = Duration::from_secs(15);

        assert!(
            !locally_observed_lease_is_expired(
                &mut observed,
                &spec,
                first_observation,
                lease_duration,
            ),
            "an old remote timestamp must not make a newly observed lease expire"
        );
        assert!(!locally_observed_lease_is_expired(
            &mut observed,
            &spec,
            first_observation + lease_duration - Duration::from_millis(1),
            lease_duration,
        ));
        assert!(locally_observed_lease_is_expired(
            &mut observed,
            &spec,
            first_observation + lease_duration,
            lease_duration,
        ));

        spec.renew_time = Some(kube_micro_time(remote_now + ChronoDuration::hours(1)));
        assert!(
            !locally_observed_lease_is_expired(
                &mut observed,
                &spec,
                first_observation + lease_duration,
                lease_duration,
            ),
            "a changed lease spec must reset the local observation time"
        );
    }

    #[test]
    fn leader_election_takes_lease_without_holder() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let lease = make_lease(None, None, None);
        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now, false, None)
            .expect("empty lease should be acquired");
        let spec = prepared.spec.expect("lease spec should be set");

        assert_eq!(spec.holder_identity.as_deref(), Some("vector-0"));
        assert_eq!(spec.lease_transitions, Some(1));
    }

    #[tokio::test]
    async fn renewal_conflict_is_retryable_within_deadline() {
        let request_count = Arc::new(AtomicUsize::new(0));
        let service = {
            let request_count = Arc::clone(&request_count);
            service_fn(move |_request: Request<Body>| {
                let request_number = request_count.fetch_add(1, Ordering::SeqCst);
                let body = if request_number == 0 {
                    serde_json::to_vec(&make_lease(Some("vector-0"), Some(Utc::now()), Some(1)))
                        .unwrap()
                } else {
                    serde_json::to_vec(&serde_json::json!({
                        "apiVersion": "v1",
                        "kind": "Status",
                        "status": "Failure",
                        "message": "the object has been modified",
                        "reason": "Conflict",
                        "code": 409,
                    }))
                    .unwrap()
                };
                let status = if request_number == 0 {
                    StatusCode::OK
                } else {
                    StatusCode::CONFLICT
                };
                async move { Ok::<_, std::io::Error>(json_response(status, body)) }
            })
        };
        let coordinator =
            LeaseCoordinator::new(Client::new(service, "default"), leader_settings("vector-0"));
        let mut last_renewal = Instant::now();
        let checkpoints = checkpoints("config", &[("all", "123")]);

        let end = renew_leadership(&coordinator, &mut last_renewal, &checkpoints).await;

        assert!(
            end.is_none(),
            "a replace conflict must be retried until the renewal deadline"
        );
        assert_eq!(request_count.load(Ordering::SeqCst), 2);
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

        let prepared = prepare_lease_update(lease, &leader_settings("vector-0"), now, true, None)
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

        let prepared = prepare_lease_update(
            lease,
            &leader_settings("vector-0"),
            now,
            false,
            Some(&updated),
        )
        .expect("self-held lease should renew");

        assert_eq!(lease_checkpoints(&prepared), Some(updated));
    }

    #[test]
    fn lease_update_omits_checkpoints_that_exceed_annotation_limit() {
        let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let mut lease = make_lease(Some("vector-0"), Some(now), Some(1));
        let annotations = lease
            .metadata
            .annotations
            .get_or_insert_with(Default::default);
        annotations.insert(
            LEGACY_WATERMARK_ANNOTATION.to_string(),
            "legacy".to_string(),
        );
        annotations.insert("example.com/other".to_string(), "keep-me".to_string());

        let oversized = checkpoints(
            "config",
            &[("all", &"x".repeat(KUBERNETES_ANNOTATIONS_SIZE_LIMIT_BYTES))],
        );
        let prepared = prepare_lease_update(
            lease,
            &leader_settings("vector-0"),
            now,
            false,
            Some(&oversized),
        )
        .expect("an oversized checkpoint must not prevent lease renewal");
        let annotations = prepared
            .metadata
            .annotations
            .expect("unrelated annotations should remain");

        assert!(!annotations.contains_key(CHECKPOINTS_ANNOTATION));
        assert!(!annotations.contains_key(LEGACY_WATERMARK_ANNOTATION));
        assert_eq!(
            annotations.get("example.com/other").map(String::as_str),
            Some("keep-me")
        );
        assert_eq!(
            prepared
                .spec
                .and_then(|spec| spec.renew_time)
                .and_then(|time| kube_timestamp_to_chrono(time.0)),
            Some(now)
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
        let updated = checkpoints("config", &[("all", "123")]);

        let prepared = prepare_lease_update(
            lease,
            &leader_settings("vector-0"),
            now,
            false,
            Some(&updated),
        )
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

        let prepared = prepare_lease_update(
            lease,
            &leader_settings("vector-0"),
            now,
            false,
            Some(&updated),
        )
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
