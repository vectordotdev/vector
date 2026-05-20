#![allow(missing_docs)]
use std::{
    sync::{
        Arc, LazyLock, Mutex, OnceLock, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use base64::prelude::{BASE64_URL_SAFE, Engine as _};
use google_cloud_auth::{
    build_errors::Error as BuildError,
    credentials::{
        AccessTokenCredentials, Builder as AdcBuilder,
        external_account::Builder as ExternalAccountBuilder,
        impersonated::Builder as ImpersonatedBuilder,
        service_account::{AccessSpecifier, Builder as ServiceAccountBuilder},
        user_account::Builder as UserAccountBuilder,
    },
    errors::CredentialsError,
};
use http::{HeaderValue, Uri, header::InvalidHeaderValue, uri::PathAndQuery};
use hyper::header::AUTHORIZATION;
use snafu::{ResultExt, Snafu};
use tokio::sync::watch;
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};

/// A GCP OAuth 2.0 scope. Constructible only via the constants exposed below
/// (or — within this crate — internally), so a typo in a sink wiring is a
/// compile error rather than a 401 at runtime. The SDK accepts any string,
/// but Vector's surface area is fixed.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Scope(&'static str);

impl Scope {
    pub const PUBSUB: Self = Self("https://www.googleapis.com/auth/pubsub");
    pub const DEVSTORAGE_READ_WRITE: Self =
        Self("https://www.googleapis.com/auth/devstorage.read_write");
    pub const LOGGING_WRITE: Self = Self("https://www.googleapis.com/auth/logging.write");
    pub const MONITORING_WRITE: Self = Self("https://www.googleapis.com/auth/monitoring.write");
    pub const MALACHITE_INGESTION: Self =
        Self("https://www.googleapis.com/auth/malachite-ingestion");
    #[cfg(test)]
    pub const COMPUTE: Self = Self("https://www.googleapis.com/auth/compute");

    const fn as_str(self) -> &'static str {
        self.0
    }
}

// Fixed refresh interval. GCP access tokens are typically ~1h, but
// Workload-Identity-Federation / impersonated-service-account flows can
// issue tokens with sub-1800s lifetimes. The SDK's public `AccessToken`
// discards the inner `expires_at`, so the loop can't be driven from
// per-token expiry; pick a fallback interval short enough to bound the
// damage for those flows. The 401 self-heal path on each request handles
// the common-case stale-token case ahead of this tick.
const TOKEN_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);
// Initial backoff after a regenerator failure. Doubles on each subsequent
// failure up to TOKEN_REFRESH_INTERVAL — i.e. during an STS outage we taper
// from one attempt every 2s up to one attempt every 5 min, instead of
// hot-spinning at 2s for the whole outage. Reset to this value on success.
const TOKEN_ERROR_RETRY_MIN: Duration = Duration::from_secs(2);

// Bound on each token-fetch network call. The SDK's `access_token()` has no
// internal timeout — without this a stuck metadata server / STS endpoint would
// hang the regenerator forever and leave the cached token to silently expire.
const FETCH_TOKEN_TIMEOUT: Duration = Duration::from_secs(30);

// Lower bound on how often `force_refresh` will actually do work. Sinks call
// it from retry hot paths on 401, which can fire many times per second under
// a real outage; rebuilding credentials that often would DoS the STS endpoint
// (and our own log volume) without buying anything.
const MIN_FORCE_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

pub const PUBSUB_URL: &str = "https://pubsub.googleapis.com";

pub static PUBSUB_ADDRESS: LazyLock<String> = LazyLock::new(|| {
    std::env::var("EMULATOR_ADDRESS").unwrap_or_else(|_| "http://localhost:8681".into())
});

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GcpError {
    #[snafu(display("Invalid GCP credentials: {}", source))]
    InvalidCredentials { source: BuildError },
    #[snafu(display("Invalid GCP API key: {}", source))]
    InvalidApiKey { source: base64::DecodeError },
    #[snafu(display("Healthcheck endpoint forbidden"))]
    HealthcheckForbidden,
    #[snafu(display("Failed to read GCP credentials file {:?}: {}", path, source))]
    ReadCredentialsFile {
        path: String,
        source: std::io::Error,
    },
    #[snafu(display("Failed to parse GCP credentials JSON: {}", source))]
    ParseCredentialsJson { source: serde_json::Error },
    #[snafu(display("Unsupported GCP credentials type: {:?}", ty))]
    UnsupportedCredentialsType { ty: String },
    #[snafu(display("Failed to get GCP OAuth token: {}", source))]
    GetToken { source: CredentialsError },
    #[snafu(display("Timed out after {:?} fetching GCP OAuth token", timeout))]
    FetchTokenTimeout { timeout: Duration },
    #[snafu(display("GCP OAuth token is not a valid HTTP header value: {}", source))]
    InvalidAuthHeader { source: InvalidHeaderValue },
}

/// Configuration of the authentication strategy for interacting with GCP services.
// TODO: We're duplicating the "either this or that" verbiage for each field because this struct gets flattened into the
// component config types, which means all that's carried over are the fields, not the type itself.
//
// Seems like we really really have it as a nested field -- i.e. `auth.api_key` -- which is a closer fit to how we do
// similar things in configuration (TLS, framing, decoding, etc.). Doing so would let us embed the type itself, and
// hoist up the common documentation bits to the docs for the type rather than the fields.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct GcpAuthConfig {
    /// An [API key][gcp_api_key].
    ///
    /// Either an API key or a path to a credentials JSON file can be specified.
    ///
    /// If both are unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
    /// filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
    /// running on. If this is not on a GCE instance, then you must define it with an API key or a credentials JSON file.
    ///
    /// [gcp_api_key]: https://cloud.google.com/docs/authentication/api-keys
    pub api_key: Option<SensitiveString>,

    /// Path to a GCP [credentials JSON file][gcp_credentials]. In addition to classic service account keys,
    /// this field accepts [Workload Identity Federation][gcp_wif] (`external_account`), authorized user,
    /// and impersonated service account credentials. The file's `type` field selects the flow.
    ///
    /// Either an API key or a path to a credentials JSON file can be specified.
    ///
    /// If both are unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
    /// filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
    /// running on. If this is not on a GCE instance, then you must define it with an API key or a credentials JSON file.
    ///
    /// [gcp_credentials]: https://cloud.google.com/docs/authentication/production#manually
    /// [gcp_wif]: https://cloud.google.com/iam/docs/workload-identity-federation
    pub credentials_path: Option<String>,

    /// Skip all authentication handling. For use with integration tests only.
    #[serde(default, skip_serializing)]
    #[configurable(metadata(docs::hidden))]
    pub skip_authentication: bool,
}

/// Install the ring rustls crypto provider as the process-wide default.
/// Idempotent: subsequent calls are no-ops. Called once before any
/// `AccessTokenCredentials` is built (the SDK pulls in `rustls` which panics
/// if no provider is installed when its TLS path runs).
fn ensure_rustls_provider_installed() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        // `install_default` returns Err if another provider was already
        // installed (e.g. aws-lc-rs from a different component). That is
        // fine; the SDK's TLS path uses whichever provider is current.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

impl GcpAuthConfig {
    pub async fn build(&self, scope: Scope) -> crate::Result<GcpAuthenticator> {
        ensure_rustls_provider_installed();
        if self.skip_authentication {
            return Ok(GcpAuthenticator::None);
        }
        let scope = scope.as_str();
        // Precedence matches the pre-OIDC-migration behavior:
        //   1. explicit `credentials_path`
        //   2. `GOOGLE_APPLICATION_CREDENTIALS` env var (treated as a path)
        //   3. `api_key`
        //   4. implicit (ADC: well-known file, then GCE metadata)
        // The env var is inspected here rather than delegated to the SDK's
        // AdcBuilder because we want it to take precedence over `api_key`
        // — operators who have both set typically expect the file-mounted
        // identity to win, since key + env is a common Kubernetes pattern
        // where the env is set by a pod template and the api_key is a
        // component-level override.
        let env_creds_path = std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok();
        let creds_path = self
            .credentials_path
            .as_deref()
            .or(env_creds_path.as_deref());
        Ok(match (creds_path, &self.api_key) {
            (Some(path), _) => GcpAuthenticator::from_file(path, scope).await?,
            (None, Some(api_key)) => GcpAuthenticator::from_api_key(api_key.inner())?,
            (None, None) => GcpAuthenticator::new_implicit(scope).await?,
        })
    }
}

#[derive(Clone, Debug)]
pub enum GcpAuthenticator {
    Credentials(Arc<InnerCreds>),
    ApiKey(Box<str>),
    None,
}

/// Where to fetch credentials from when (re)building. Kept so that each refresh
/// rebuilds a fresh `AccessTokenCredentials`, which discards any cached token
/// inside the SDK and re-reads the source file (so configmap/IAM-controller
/// rotations are picked up without a pod restart).
#[derive(Clone, Debug)]
enum CredSource {
    File { path: String, scope: String },
    Implicit { scope: String },
}

pub struct InnerCreds {
    source: CredSource,
    state: RwLock<CredState>,
    /// Wall-clock time of the most recent *successful* refresh from any
    /// path (periodic regenerator or `force_refresh`). `force_refresh`
    /// consults this to decide whether `MIN_FORCE_REFRESH_INTERVAL` has
    /// elapsed; a failed attempt does NOT advance the timestamp, so a
    /// transient STS failure won't silently throttle out the next 401-driven
    /// retry. Concurrent attempts during a single STS burst are coalesced
    /// by `refresh_in_flight` below.
    last_refresh_success: Mutex<Option<Instant>>,
    /// Set while a `force_refresh` attempt is in flight so concurrent 401s
    /// in a burst don't all hit STS in parallel. The periodic regenerator
    /// does not use this guard — it runs on a single task and never overlaps
    /// itself.
    refresh_in_flight: AtomicBool,
}

/// RAII clear of `refresh_in_flight` on drop, so a panic in the refresh task
/// can't strand the flag set.
struct InFlightGuard<'a>(&'a AtomicBool);
impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Per-refresh snapshot. Held under a single `RwLock` so readers always see a
/// coherent (token, cred_type, project_id) triple. `auth_header` is the
/// `Authorization` `HeaderValue` produced by the SDK's `headers()` API,
/// already formatted with the correct scheme (Bearer or otherwise) — `apply`
/// just clones it onto the outgoing request.
struct CredState {
    auth_header: HeaderValue,
    cred_type: String,
    project_id: Option<String>,
}

impl std::fmt::Debug for InnerCreds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.read().unwrap();
        f.debug_struct("InnerCreds")
            .field("source", &self.source)
            .field("cred_type", &state.cred_type)
            .field("project_id", &state.project_id)
            .finish_non_exhaustive()
    }
}

impl GcpAuthenticator {
    async fn from_file(path: &str, scope: &str) -> crate::Result<Self> {
        let (creds, cred_type, project_id) = build_from_file(path, scope).await?;
        let auth_header = fetch_auth_header(&creds, &cred_type, project_id.as_deref()).await?;
        // `creds` is dropped here; subsequent fetches rebuild via `CredSource::File`.
        Ok(Self::Credentials(Arc::new(InnerCreds {
            source: CredSource::File {
                path: path.to_string(),
                scope: scope.to_string(),
            },
            state: RwLock::new(CredState {
                auth_header,
                cred_type,
                project_id,
            }),
            last_refresh_success: Mutex::new(Some(Instant::now())),
            refresh_in_flight: AtomicBool::new(false),
        })))
    }

    async fn new_implicit(scope: &str) -> crate::Result<Self> {
        let creds = build_implicit(scope)?;
        let auth_header = fetch_auth_header(&creds, "application_default", None).await?;
        Ok(Self::Credentials(Arc::new(InnerCreds {
            source: CredSource::Implicit {
                scope: scope.to_string(),
            },
            state: RwLock::new(CredState {
                auth_header,
                cred_type: "application_default".into(),
                project_id: None,
            }),
            last_refresh_success: Mutex::new(Some(Instant::now())),
            refresh_in_flight: AtomicBool::new(false),
        })))
    }

    fn from_api_key(api_key: &str) -> crate::Result<Self> {
        BASE64_URL_SAFE
            .decode(api_key)
            .context(InvalidApiKeySnafu)?;
        Ok(Self::ApiKey(api_key.into()))
    }

    pub fn apply<T>(&self, request: &mut http::Request<T>) {
        if let Some(header) = self.auth_header() {
            request.headers_mut().insert(AUTHORIZATION, header);
        }
        self.apply_uri(request.uri_mut());
    }

    /// Cached `Authorization` `HeaderValue` for callers that build their own
    /// request types (e.g. tonic `MetadataValue` in the pubsub source).
    /// Returns `None` for the `ApiKey` and `None` variants — those callers
    /// either don't need a header or attach the key via the URI.
    pub fn auth_header(&self) -> Option<HeaderValue> {
        match self {
            Self::Credentials(inner) => Some(inner.state.read().unwrap().auth_header.clone()),
            Self::ApiKey(_) | Self::None => None,
        }
    }

    pub fn apply_uri(&self, uri: &mut Uri) {
        match self {
            Self::Credentials(_) | Self::None => (),
            Self::ApiKey(api_key) => {
                let mut parts = uri.clone().into_parts();
                let path = parts
                    .path_and_query
                    .as_ref()
                    .map_or("/", PathAndQuery::path);
                let paq = format!("{path}?key={api_key}");
                // The API key is verified above to only contain
                // URL-safe characters. That key is added to a path
                // that came from a successfully parsed URI. As such,
                // re-parsing the string cannot fail.
                parts.path_and_query =
                    Some(paq.parse().expect("Could not re-parse path and query"));
                *uri = Uri::from_parts(parts).expect("Could not re-parse URL");
            }
        }
    }

    /// Spawn the background token-refresh task. Most call sites (the sinks)
    /// want this — they re-read the cached token on every request via
    /// `apply()` and don't need an explicit "token rotated" signal.
    ///
    /// Calling this multiple times on the same `GcpAuthenticator` spawns
    /// multiple regenerator tasks, so each component should call it exactly
    /// once during sink/source build.
    pub fn start_background_refresh(&self) {
        // Drop the receiver: for the `Credentials` variant the task ignores
        // it; for the `ApiKey` / `None` variants the task is a no-op that
        // exits as soon as the last receiver drops, which is fine.
        let _ = self.subscribe_token_rotation();
    }

    /// Spawn the background token-refresh task and return a watch receiver
    /// that fires on each successful refresh. Use this only when the caller
    /// needs an explicit token-rotation signal — today only the
    /// `gcp_pubsub` source (which uses it to break/recycle the gRPC stream).
    pub fn subscribe_token_rotation(&self) -> watch::Receiver<()> {
        let (sender, receiver) = watch::channel(());
        tokio::spawn(self.clone().token_regenerator(sender));
        receiver
    }

    /// Out-of-band credential rebuild. Called by sink retry logic when GCS
    /// returns 401 to convert a permanent stale-token state into a one-shot
    /// self-heal. Internally throttled to `MIN_FORCE_REFRESH_INTERVAL`, so
    /// callers can fire it on every 401 without coordination.
    pub async fn force_refresh(&self) {
        if let Self::Credentials(inner) = self {
            inner.force_refresh().await;
        }
    }

    async fn token_regenerator(self, sender: watch::Sender<()>) {
        match self {
            Self::Credentials(inner) => {
                let mut deadline = TOKEN_REFRESH_INTERVAL;
                let mut error_backoff = TOKEN_ERROR_RETRY_MIN;
                loop {
                    debug!(
                        deadline = deadline.as_secs(),
                        "Sleeping before refreshing GCP authentication token.",
                    );
                    tokio::time::sleep(deadline).await;
                    match inner.regenerate_token().await {
                        Ok(()) => {
                            // Update the shared success timestamp so a 401
                            // burst right after this tick will see the
                            // force_refresh throttle and skip — periodic
                            // and 401-driven paths share one throttle window.
                            *inner.last_refresh_success.lock().unwrap() = Some(Instant::now());
                            sender.send_replace(());
                            debug!("GCP authentication token renewed.");
                            deadline = TOKEN_REFRESH_INTERVAL;
                            error_backoff = TOKEN_ERROR_RETRY_MIN;
                        }
                        Err(error) => {
                            error!(
                                message = "Failed to update GCP authentication token.",
                                next_retry_secs = error_backoff.as_secs(),
                                %error
                            );
                            deadline = error_backoff;
                            // Double until we hit TOKEN_REFRESH_INTERVAL,
                            // then plateau. saturating_mul guards Duration
                            // overflow even though we'll cap long before it.
                            error_backoff =
                                error_backoff.saturating_mul(2).min(TOKEN_REFRESH_INTERVAL);
                        }
                    }
                }
            }
            Self::ApiKey(_) | Self::None => {
                // This keeps the sender end of the watch open without
                // actually sending anything, effectively creating an
                // empty watch stream.
                sender.closed().await
            }
        }
    }
}

impl InnerCreds {
    /// Build a fresh `AccessTokenCredentials` and fetch a token through it.
    /// Both steps happen every refresh: rebuilding discards any token cached
    /// inside the SDK, and re-reads the credentials file from disk (so
    /// IAM-controller / kubelet rotations are picked up without restarting
    /// the pod).
    async fn regenerate_token(&self) -> crate::Result<()> {
        let (creds, cred_type, project_id) = match &self.source {
            CredSource::File { path, scope } => build_from_file(path, scope).await?,
            CredSource::Implicit { scope } => (
                build_implicit(scope)?,
                "application_default".to_string(),
                None,
            ),
        };
        let auth_header = fetch_auth_header(&creds, &cred_type, project_id.as_deref()).await?;
        // Single write — readers see a coherent (auth_header, cred_type,
        // project_id) snapshot. Never held across an await.
        *self.state.write().unwrap() = CredState {
            auth_header,
            cred_type,
            project_id,
        };
        Ok(())
    }

    async fn force_refresh(&self) {
        // Coalesce concurrent 401s: only one task hits STS for the burst.
        // The losers early-return — the winner's result will update the
        // cached token before their next request retry runs.
        if self.refresh_in_flight.swap(true, Ordering::SeqCst) {
            debug!("Skipping forced GCP credentials refresh — another attempt is in flight.");
            return;
        }
        let _guard = InFlightGuard(&self.refresh_in_flight);

        // Throttle: skip if the periodic regenerator OR a prior force_refresh
        // succeeded recently. A failed prior attempt does NOT advance the
        // timestamp, so a transient STS failure does not silently throttle
        // out subsequent 401s.
        {
            let last = self.last_refresh_success.lock().unwrap();
            if let Some(prev) = *last
                && Instant::now().duration_since(prev) < MIN_FORCE_REFRESH_INTERVAL
            {
                debug!(
                    "Skipping forced GCP credentials refresh — last success was within throttle window."
                );
                return;
            }
        }

        match self.regenerate_token().await {
            Ok(()) => {
                *self.last_refresh_success.lock().unwrap() = Some(Instant::now());
                debug!("Forced GCP credentials refresh succeeded.");
            }
            Err(error) => error!(
                message = "Forced GCP credentials refresh failed.",
                %error,
            ),
        }
    }
}

/// Fetch the OAuth access token and build the `Authorization` `HeaderValue`
/// for it. The SDK's `headers()` API would also produce a `HeaderValue` but
/// it returns the `http 1.x` variant; Vector's request types are still on
/// `http 0.2`, so we go through the SDK's `access_token()` (which is just
/// the raw token string) and build the header here.
///
/// The `"Bearer "` scheme is hard-coded: the SDK only models bearer tokens
/// (its `AccessToken` type has no scheme field) and GCP only issues bearer
/// access tokens. If either ever changes we'd need an SDK update too.
///
/// The `HeaderValue` is built and validated here so any failure surfaces at
/// refresh time (with a clear error). `apply()` then just clones the cached
/// value per request — no per-request `.parse().unwrap()` panic risk.
async fn fetch_auth_header(
    creds: &AccessTokenCredentials,
    cred_type: &str,
    project_id: Option<&str>,
) -> crate::Result<HeaderValue> {
    debug!(
        cred_type = %cred_type,
        project_id = ?project_id,
        "Fetching GCP authentication token.",
    );
    let token = tokio::time::timeout(FETCH_TOKEN_TIMEOUT, creds.access_token())
        .await
        .map_err(|_| GcpError::FetchTokenTimeout {
            timeout: FETCH_TOKEN_TIMEOUT,
        })?
        .context(GetTokenSnafu)?;
    Ok(
        HeaderValue::from_str(&format!("Bearer {}", token.token))
            .context(InvalidAuthHeaderSnafu)?,
    )
}

async fn build_from_file(
    path: &str,
    scope: &str,
) -> crate::Result<(AccessTokenCredentials, String, Option<String>)> {
    let bytes = tokio::fs::read(path)
        .await
        .context(ReadCredentialsFileSnafu {
            path: path.to_string(),
        })?;
    // `from_slice` runs synchronously on the runtime worker thread. Credentials
    // files are typically <1 KiB so this is microseconds — not worth a
    // spawn_blocking. Every refresh re-parses; if the files ever grow into the
    // tens-of-KB range, move this to spawn_blocking.
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).context(ParseCredentialsJsonSnafu)?;
    let cred_type = json
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let project_id = json
        .get("project_id")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let creds = build_credentials_from_json(&json, scope)?;
    Ok((creds, cred_type, project_id))
}

fn build_implicit(scope: &str) -> crate::Result<AccessTokenCredentials> {
    AdcBuilder::default()
        .with_scopes([scope])
        .build_access_token_credentials()
        .context(InvalidCredentialsSnafu)
        .map_err(Into::into)
}

fn build_credentials_from_json(
    json: &serde_json::Value,
    scope: &str,
) -> crate::Result<AccessTokenCredentials> {
    let ty = json
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GcpError::UnsupportedCredentialsType {
            ty: "<missing>".into(),
        })?
        .to_string();
    let scopes = [scope.to_string()];
    let json_owned = json.clone();
    let creds = match ty.as_str() {
        "service_account" => ServiceAccountBuilder::new(json_owned)
            .with_access_specifier(AccessSpecifier::from_scopes(scopes))
            .build_access_token_credentials(),
        "external_account" => ExternalAccountBuilder::new(json_owned)
            .with_scopes(scopes)
            .build_access_token_credentials(),
        "authorized_user" => UserAccountBuilder::new(json_owned)
            .with_scopes(scopes)
            .build_access_token_credentials(),
        "impersonated_service_account" => ImpersonatedBuilder::new(json_owned)
            .with_scopes(scopes)
            .build_access_token_credentials(),
        other => {
            return Err(GcpError::UnsupportedCredentialsType {
                ty: other.to_string(),
            }
            .into());
        }
    };
    creds.context(InvalidCredentialsSnafu).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_downcast_matches;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // The pre-OIDC test exercised the implicit-creds path
    // (`build("").await`), which now goes through the SDK's ADC builder.
    // Off-GCE that hits a DNS lookup for `metadata.google.internal`, making
    // the test fail in any CI environment without GCE metadata reachable.
    // We replaced it with file-based failure-mode tests that don't depend
    // on the SDK's network behavior — they exercise `build_from_file` /
    // `build_credentials_from_json` directly, which is where the new OIDC
    // code paths live.

    /// `credentials_path` pointing at a path that doesn't exist should
    /// produce a `ReadCredentialsFile` error rather than panicking or
    /// surfacing an opaque SDK error.
    #[tokio::test]
    async fn fails_missing_credentials_file() {
        let error = build_auth(
            r#"
                credentials_path = "/nonexistent/path/that/should/not/exist"
            "#,
        )
        .await
        .expect_err("build failed to error");
        assert_downcast_matches!(error, GcpError, GcpError::ReadCredentialsFile { .. });
    }

    /// Malformed JSON in the credentials file should produce
    /// `ParseCredentialsJson`.
    #[tokio::test]
    async fn fails_malformed_credentials_json() {
        let mut file = NamedTempFile::new().expect("create temp file");
        file.write_all(b"{this is not json")
            .expect("write temp file");
        let path = file.path().to_str().expect("path utf8");

        let error = build_auth(&format!(r#"credentials_path = "{path}""#))
            .await
            .expect_err("build failed to error");
        assert_downcast_matches!(error, GcpError, GcpError::ParseCredentialsJson { .. });
    }

    /// JSON without a `type` field should produce
    /// `UnsupportedCredentialsType` with `<missing>` as the type name.
    #[tokio::test]
    async fn fails_missing_type_field() {
        let mut file = NamedTempFile::new().expect("create temp file");
        file.write_all(br#"{"project_id": "p"}"#)
            .expect("write temp file");
        let path = file.path().to_str().expect("path utf8");

        let error = build_auth(&format!(r#"credentials_path = "{path}""#))
            .await
            .expect_err("build failed to error");
        let err = error.downcast::<GcpError>().expect("not a GcpError");
        match *err {
            GcpError::UnsupportedCredentialsType { ty } => assert_eq!(ty, "<missing>"),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    /// JSON with an unknown `type` should produce `UnsupportedCredentialsType`
    /// echoing the offending type name.
    #[tokio::test]
    async fn fails_unsupported_type() {
        let mut file = NamedTempFile::new().expect("create temp file");
        file.write_all(br#"{"type": "magic_beans"}"#)
            .expect("write temp file");
        let path = file.path().to_str().expect("path utf8");

        let error = build_auth(&format!(r#"credentials_path = "{path}""#))
            .await
            .expect_err("build failed to error");
        let err = error.downcast::<GcpError>().expect("not a GcpError");
        match *err {
            GcpError::UnsupportedCredentialsType { ty } => assert_eq!(ty, "magic_beans"),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[tokio::test]
    async fn skip_authentication() {
        let auth = build_auth(
            r#"
                skip_authentication = true
                api_key = "testing"
            "#,
        )
        .await
        .expect("build_auth failed");
        assert!(matches!(auth, GcpAuthenticator::None));
    }

    #[tokio::test]
    async fn uses_api_key() {
        let key = crate::test_util::random_string(16);

        let auth = build_auth(&format!(r#"api_key = "{key}""#))
            .await
            .expect("build_auth failed");
        assert!(matches!(auth, GcpAuthenticator::ApiKey(..)));

        assert_eq!(
            apply_uri(&auth, "http://example.com"),
            format!("http://example.com/?key={key}")
        );
        assert_eq!(
            apply_uri(&auth, "http://example.com/"),
            format!("http://example.com/?key={key}")
        );
        assert_eq!(
            apply_uri(&auth, "http://example.com/path"),
            format!("http://example.com/path?key={key}")
        );
        assert_eq!(
            apply_uri(&auth, "http://example.com/path1/"),
            format!("http://example.com/path1/?key={key}")
        );
    }

    #[tokio::test]
    async fn fails_bad_api_key() {
        let error = build_auth(r#"api_key = "abc%xyz""#)
            .await
            .expect_err("build failed to error");
        assert_downcast_matches!(error, GcpError, GcpError::InvalidApiKey { .. });
    }

    fn apply_uri(auth: &GcpAuthenticator, uri: &str) -> String {
        let mut uri: Uri = uri.parse().unwrap();
        auth.apply_uri(&mut uri);
        uri.to_string()
    }

    async fn build_auth(toml: &str) -> crate::Result<GcpAuthenticator> {
        let config: GcpAuthConfig = toml::from_str(toml).expect("Invalid TOML");
        config.build(Scope::COMPUTE).await
    }
}
