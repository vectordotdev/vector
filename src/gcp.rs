#![allow(missing_docs)]
use std::{
    sync::{Arc, LazyLock, Mutex, RwLock},
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
use http::{Uri, uri::PathAndQuery};
use hyper::header::AUTHORIZATION;
use snafu::{ResultExt, Snafu};
use tokio::sync::watch;
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};

// GCP OAuth 2.0 scopes used by Vector components. String-typed because the
// `google-cloud-auth` SDK accepts any scope string.
pub const SCOPE_PUBSUB: &str = "https://www.googleapis.com/auth/pubsub";
pub const SCOPE_DEVSTORAGE_READ_WRITE: &str =
    "https://www.googleapis.com/auth/devstorage.read_write";
pub const SCOPE_LOGGING_WRITE: &str = "https://www.googleapis.com/auth/logging.write";
pub const SCOPE_MONITORING_WRITE: &str = "https://www.googleapis.com/auth/monitoring.write";
pub const SCOPE_MALACHITE_INGESTION: &str = "https://www.googleapis.com/auth/malachite-ingestion";
#[cfg(test)]
pub const SCOPE_COMPUTE: &str = "https://www.googleapis.com/auth/compute";

// Fixed refresh interval. GCP access tokens are typically ~1h; refreshing
// every 30 min keeps a fresh token available without driving the loop from
// per-token expiry metadata (the new SDK does not expose it).
const TOKEN_REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60);
const TOKEN_ERROR_RETRY: Duration = Duration::from_secs(2);

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
    /// Either an API key or a path to a service account credentials JSON file can be specified.
    ///
    /// If both are unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
    /// filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
    /// running on. If this is not on a GCE instance, then you must define it with an API key or service account
    /// credentials JSON file.
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

impl GcpAuthConfig {
    pub async fn build(&self, scope: &str) -> crate::Result<GcpAuthenticator> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Ok(if self.skip_authentication {
            GcpAuthenticator::None
        } else {
            match (&self.credentials_path, &self.api_key) {
                (Some(path), _) => GcpAuthenticator::from_file(path, scope).await?,
                (_, Some(api_key)) => GcpAuthenticator::from_api_key(api_key.inner())?,
                (None, None) => GcpAuthenticator::new_implicit(scope).await?,
            }
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
    token: RwLock<String>,
    cred_type: RwLock<String>,
    project_id: RwLock<Option<String>>,
    last_force_refresh: Mutex<Option<Instant>>,
}

impl std::fmt::Debug for InnerCreds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InnerCreds")
            .field("source", &self.source)
            .field("cred_type", &*self.cred_type.read().unwrap())
            .field("project_id", &*self.project_id.read().unwrap())
            .finish_non_exhaustive()
    }
}

impl GcpAuthenticator {
    async fn from_file(path: &str, scope: &str) -> crate::Result<Self> {
        let (creds, cred_type, project_id) = build_from_file(path, scope).await?;
        let initial = fetch_token(&creds, &cred_type, project_id.as_deref()).await?;
        // `creds` is dropped here; subsequent fetches rebuild via `CredSource::File`.
        Ok(Self::Credentials(Arc::new(InnerCreds {
            source: CredSource::File {
                path: path.to_string(),
                scope: scope.to_string(),
            },
            token: RwLock::new(initial),
            cred_type: RwLock::new(cred_type),
            project_id: RwLock::new(project_id),
            last_force_refresh: Mutex::new(None),
        })))
    }

    async fn new_implicit(scope: &str) -> crate::Result<Self> {
        let creds = build_implicit(scope)?;
        let initial = fetch_token(&creds, "application_default", None).await?;
        Ok(Self::Credentials(Arc::new(InnerCreds {
            source: CredSource::Implicit {
                scope: scope.to_string(),
            },
            token: RwLock::new(initial),
            cred_type: RwLock::new("application_default".into()),
            project_id: RwLock::new(None),
            last_force_refresh: Mutex::new(None),
        })))
    }

    fn from_api_key(api_key: &str) -> crate::Result<Self> {
        BASE64_URL_SAFE
            .decode(api_key)
            .context(InvalidApiKeySnafu)?;
        Ok(Self::ApiKey(api_key.into()))
    }

    pub fn make_token(&self) -> Option<String> {
        match self {
            Self::Credentials(inner) => Some(inner.make_token()),
            Self::ApiKey(_) | Self::None => None,
        }
    }

    pub fn apply<T>(&self, request: &mut http::Request<T>) {
        if let Some(token) = self.make_token() {
            request
                .headers_mut()
                .insert(AUTHORIZATION, token.parse().unwrap());
        }
        self.apply_uri(request.uri_mut());
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

    pub fn spawn_regenerate_token(&self) -> watch::Receiver<()> {
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
                loop {
                    debug!(
                        deadline = deadline.as_secs(),
                        "Sleeping before refreshing GCP authentication token.",
                    );
                    tokio::time::sleep(deadline).await;
                    match inner.regenerate_token().await {
                        Ok(()) => {
                            sender.send_replace(());
                            debug!("GCP authentication token renewed.");
                            deadline = TOKEN_REFRESH_INTERVAL;
                        }
                        Err(error) => {
                            error!(
                                message = "Failed to update GCP authentication token.",
                                %error
                            );
                            deadline = TOKEN_ERROR_RETRY;
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
        let token = fetch_token(&creds, &cred_type, project_id.as_deref()).await?;
        // Briefly hold each write lock; never across an await.
        *self.token.write().unwrap() = token;
        *self.cred_type.write().unwrap() = cred_type;
        *self.project_id.write().unwrap() = project_id;
        Ok(())
    }

    async fn force_refresh(&self) {
        {
            let mut last = self.last_force_refresh.lock().unwrap();
            let now = Instant::now();
            if let Some(prev) = *last
                && now.duration_since(prev) < MIN_FORCE_REFRESH_INTERVAL
            {
                debug!("Skipping forced GCP credentials refresh — within throttle window.");
                return;
            }
            *last = Some(now);
        }
        match self.regenerate_token().await {
            Ok(()) => debug!("Forced GCP credentials refresh succeeded."),
            Err(error) => error!(
                message = "Forced GCP credentials refresh failed.",
                %error,
            ),
        }
    }

    fn make_token(&self) -> String {
        let token = self.token.read().unwrap();
        format!("Bearer {}", *token)
    }
}

async fn fetch_token(
    creds: &AccessTokenCredentials,
    cred_type: &str,
    project_id: Option<&str>,
) -> crate::Result<String> {
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
    Ok(token.token)
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

    #[tokio::test]
    async fn fails_missing_creds() {
        let error = build_auth("").await.expect_err("build failed to error");
        assert_downcast_matches!(error, GcpError, GcpError::InvalidCredentials { .. });
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
        config.build(SCOPE_COMPUTE).await
    }
}
