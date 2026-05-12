#![allow(missing_docs)]
use std::{
    sync::{Arc, LazyLock, RwLock},
    time::Duration,
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

pub struct InnerCreds {
    creds: AccessTokenCredentials,
    token: RwLock<String>,
    cred_type: String,
    project_id: Option<String>,
}

impl std::fmt::Debug for InnerCreds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InnerCreds")
            .field("cred_type", &self.cred_type)
            .field("project_id", &self.project_id)
            .finish_non_exhaustive()
    }
}

impl GcpAuthenticator {
    async fn from_file(path: &str, scope: &str) -> crate::Result<Self> {
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
        Self::from_credentials(creds, cred_type, project_id).await
    }

    async fn new_implicit(scope: &str) -> crate::Result<Self> {
        let creds = AdcBuilder::default()
            .with_scopes([scope])
            .build_access_token_credentials()
            .context(InvalidCredentialsSnafu)?;
        Self::from_credentials(creds, "application_default".into(), None).await
    }

    async fn from_credentials(
        creds: AccessTokenCredentials,
        cred_type: String,
        project_id: Option<String>,
    ) -> crate::Result<Self> {
        let initial = fetch_token(&creds, &cred_type, project_id.as_deref()).await?;
        let inner = InnerCreds {
            creds,
            token: RwLock::new(initial),
            cred_type,
            project_id,
        };
        Ok(Self::Credentials(Arc::new(inner)))
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
    async fn regenerate_token(&self) -> crate::Result<()> {
        let token = fetch_token(&self.creds, &self.cred_type, self.project_id.as_deref()).await?;
        *self.token.write().unwrap() = token;
        Ok(())
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
    let token = creds.access_token().await.context(GetTokenSnafu)?;
    Ok(token.token)
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
