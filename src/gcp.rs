#![allow(missing_docs)]
use std::{
    sync::{Arc, LazyLock, RwLock},
    time::Duration,
};

use base64::prelude::{BASE64_URL_SAFE, Engine as _};
pub use goauth::scopes::Scope;
use goauth::{
    GoErr,
    auth::{JwtClaims, Token, TokenErr},
    credentials::Credentials,
};
use google_cloud_auth::{
    credentials::{AccessTokenCredentials, external_account},
    errors::CredentialsError,
};
use http::{Uri, header::AUTHORIZATION, uri::PathAndQuery};
use http_body_util::{BodyExt as _, Collected};
use smpl_jwt::Jwt;
use snafu::{ResultExt, Snafu};
use tokio::sync::watch;
use vector_http::{HttpError, empty_body};
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};

use crate::{config::ProxyConfig, http::HttpClient, tls::MaybeTlsSettings};

const SERVICE_ACCOUNT_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

// See https://cloud.google.com/compute/docs/access/authenticate-workloads#applications
const METADATA_TOKEN_EXPIRY_MARGIN_SECS: u64 = 200;

const METADATA_TOKEN_ERROR_RETRY_SECS: u64 = 2;

const EXTERNAL_ACCOUNT_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

const GOOGLE_APPLICATION_CREDENTIALS_ENV_VAR: &str = "GOOGLE_APPLICATION_CREDENTIALS";

pub const BIGQUERY_STORAGE_URL: &str = "https://bigquerystorage.googleapis.com:443";
pub const PUBSUB_URL: &str = "https://pubsub.googleapis.com";

pub static PUBSUB_ADDRESS: LazyLock<String> = LazyLock::new(|| {
    std::env::var("EMULATOR_ADDRESS").unwrap_or_else(|_| "http://localhost:8681".into())
});

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GcpError {
    #[snafu(display("This requires one of api_key or credentials_path to be defined"))]
    MissingAuth,
    #[snafu(display("Invalid GCP credentials: {}", source))]
    InvalidCredentials { source: GoErr },
    #[snafu(display("Invalid GCP API key: {}", source))]
    InvalidApiKey { source: base64::DecodeError },
    #[snafu(display("Healthcheck endpoint forbidden"))]
    HealthcheckForbidden,
    #[snafu(display("Invalid RSA key in GCP credentials: {}", source))]
    InvalidRsaKey { source: GoErr },
    #[snafu(display("Failed to get OAuth token: {}", source))]
    GetToken { source: GoErr },
    #[snafu(display("Failed to get OAuth token text: {}", source))]
    GetTokenBytes { source: hyper::Error },
    #[snafu(display("Failed to get implicit GCP token: {}", source))]
    GetImplicitToken { source: HttpError },
    #[snafu(display("Failed to parse OAuth token JSON: {}", source))]
    TokenFromJson { source: TokenErr },
    #[snafu(display("Failed to parse OAuth token JSON text: {}", source))]
    TokenJsonFromStr { source: serde_json::Error },
    #[snafu(display("Failed to build HTTP client: {}", source))]
    BuildHttpClient { source: crate::Error },
    #[snafu(display("Invalid GCP external account credentials: {}", source))]
    BuildExternalAccount {
        source: google_cloud_auth::build_errors::Error,
    },
    #[snafu(display("Failed to get GCP external account token: {reason}"))]
    GetExternalAccountToken { reason: String },
}

/// Configuration for GCP credentials and authentication.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct GcpCredentialsConfig {
    /// Path to a [service account][gcp_service_account_credentials] or
    /// [Workload Identity Federation][gcp_workload_identity_federation] (`external_account`)
    /// credentials JSON file.
    ///
    /// If unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked. If that
    /// is also unset, Vector will attempt to fetch credentials from the Compute Engine metadata
    /// service if running on a GCE instance.
    ///
    /// Token requests for these credentials do not use the component's `proxy` settings. To send them
    /// through a proxy, set the `HTTPS_PROXY` (or `HTTP_PROXY`) environment variable.
    ///
    /// [gcp_service_account_credentials]: https://cloud.google.com/docs/authentication/production#manually
    /// [gcp_workload_identity_federation]: https://cloud.google.com/iam/docs/workload-identity-federation
    pub credentials_path: Option<String>,

    /// Skip all authentication handling. For use with integration tests only.
    #[serde(default, skip_serializing)]
    #[configurable(metadata(docs::hidden))]
    pub skip_authentication: bool,
}

impl GcpCredentialsConfig {
    /// # Errors
    /// Returns an error if credentials are invalid or token retrieval fails.
    pub async fn build(
        &self,
        scope: Scope,
        api_key: Option<&SensitiveString>,
    ) -> crate::Result<GcpAuthenticator> {
        Ok(if self.skip_authentication {
            GcpAuthenticator::None
        } else {
            let gap = std::env::var(GOOGLE_APPLICATION_CREDENTIALS_ENV_VAR).ok();
            let creds_path = self.credentials_path.as_ref().or(gap.as_ref());
            match (creds_path, api_key) {
                (Some(path), _) => GcpAuthenticator::from_file(path, scope).await?,
                (None, Some(api_key)) => GcpAuthenticator::from_api_key(api_key.inner())?,
                (None, None) => GcpAuthenticator::new_implicit().await?,
            }
        })
    }
}

/// Configuration of the authentication strategy for interacting with GCP services.
// TODO: We're duplicating the "either this or that" verbiage for each field because this struct gets flattened into the
// component config types, which means all that's carried over are the fields, not the type itself.
//
// Seems like we really really have it as a nested field -- i.e. `auth.api_key` -- which is a closer fit to how we do
// similar things in configuration (TLS, framing, decoding, etc.). Doing so would let us embed the type itself, and
// hoist up the common documentation bits to the docs for the type rather than the fields.
// NOTE: For new GCP components, consider using `GcpCredentialsConfig` instead. Not all GCP services
// support API keys (e.g., Cloud Storage); see issue reference: https://github.com/vectordotdev/vector/issues/20927.
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

    /// Path to a [service account][gcp_service_account_credentials] or
    /// [Workload Identity Federation][gcp_workload_identity_federation] (`external_account`)
    /// credentials JSON file.
    ///
    /// Either an API key or a path to a service account credentials JSON file can be specified.
    ///
    /// If both are unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
    /// filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
    /// running on. If this is not on a GCE instance, then you must define it with an API key or service account
    /// credentials JSON file.
    ///
    /// Token requests for these credentials do not use the component's `proxy` settings. To send them
    /// through a proxy, set the `HTTPS_PROXY` (or `HTTP_PROXY`) environment variable.
    ///
    /// [gcp_service_account_credentials]: https://cloud.google.com/docs/authentication/production#manually
    /// [gcp_workload_identity_federation]: https://cloud.google.com/iam/docs/workload-identity-federation
    pub credentials_path: Option<String>,

    /// Skip all authentication handling. For use with integration tests only.
    #[serde(default, skip_serializing)]
    #[configurable(metadata(docs::hidden))]
    pub skip_authentication: bool,
}

impl GcpAuthConfig {
    /// # Errors
    /// Returns an error if credentials or the API key are invalid, or token retrieval fails.
    pub async fn build(&self, scope: Scope) -> crate::Result<GcpAuthenticator> {
        Ok(if self.skip_authentication {
            GcpAuthenticator::None
        } else {
            let gap = std::env::var(GOOGLE_APPLICATION_CREDENTIALS_ENV_VAR).ok();
            let creds_path = self.credentials_path.as_ref().or(gap.as_ref());
            match (&creds_path, &self.api_key) {
                (Some(path), _) => GcpAuthenticator::from_file(path, scope).await?,
                (None, Some(api_key)) => GcpAuthenticator::from_api_key(api_key.inner())?,
                (None, None) => GcpAuthenticator::new_implicit().await?,
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

#[derive(Debug)]
pub struct InnerCreds {
    source: TokenSource,
    token: RwLock<FetchedToken>,
}

#[derive(Debug)]
enum TokenSource {
    ServiceAccount(Box<Credentials>, Scope),
    Metadata,
    ExternalAccount(ExternalAccount),
}

/// Workload Identity Federation credentials, handled by `google-cloud-auth`.
#[derive(Debug)]
struct ExternalAccount {
    json: serde_json::Value,
    scope: String,
    creds: RwLock<AccessTokenCredentials>,
}

#[derive(Debug)]
struct FetchedToken {
    /// Value of the `Authorization` header.
    header: String,
    refresh_in: Duration,
}

impl GcpAuthenticator {
    async fn from_file(path: &str, scope: Scope) -> crate::Result<Self> {
        let source = match external_account_json(path) {
            Some(json) => TokenSource::ExternalAccount(ExternalAccount::new(json, scope.url())?),
            None => TokenSource::ServiceAccount(
                Box::new(Credentials::from_file(path).context(InvalidCredentialsSnafu)?),
                scope,
            ),
        };
        InnerCreds::authenticator(source).await
    }

    async fn new_implicit() -> crate::Result<Self> {
        InnerCreds::authenticator(TokenSource::Metadata).await
    }

    fn from_api_key(api_key: &str) -> crate::Result<Self> {
        BASE64_URL_SAFE
            .decode(api_key)
            .context(InvalidApiKeySnafu)?;
        Ok(Self::ApiKey(api_key.into()))
    }

    #[must_use]
    pub fn make_token(&self) -> Option<String> {
        match self {
            Self::Credentials(inner) => Some(inner.make_token()),
            Self::ApiKey(_) | Self::None => None,
        }
    }

    /// # Panics
    /// Panics if the token lock is poisoned, the token is not a valid header value, or
    /// adding the API key produces an invalid URI.
    pub fn apply<T>(&self, request: &mut http::Request<T>) {
        if let Some(token) = self.make_token() {
            request
                .headers_mut()
                .insert(AUTHORIZATION, token.parse().unwrap());
        }
        self.apply_uri(request.uri_mut());
    }

    /// # Panics
    /// Panics if adding the API key produces an invalid URI.
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

    #[expect(
        clippy::must_use_candidate,
        reason = "Refresh subscription is optional"
    )]
    pub fn spawn_regenerate_token(&self) -> watch::Receiver<()> {
        let (sender, receiver) = watch::channel(());
        crate::spawn_in_current_span(self.clone().token_regenerator(sender));
        receiver
    }

    async fn token_regenerator(self, sender: watch::Sender<()>) {
        match self {
            Self::Credentials(inner) => {
                let mut deadline = inner.token.read().unwrap().refresh_in;
                loop {
                    debug!(
                        deadline = deadline.as_secs(),
                        "Sleeping before refreshing GCP authentication token.",
                    );
                    tokio::time::sleep(deadline).await;
                    match inner.regenerate_token().await {
                        Ok(changed) => {
                            // Subscribers such as the Pub/Sub source reconnect on every
                            // notification, so only notify when the token actually changed.
                            if changed {
                                sender.send_replace(());
                                debug!("GCP authentication token renewed.");
                            }
                            // Rather than an expected fresh token, the Metadata Server may return
                            // the same (cached) token during the last 300 seconds of its lifetime.
                            // This scenario is handled by retrying the token refresh after the
                            // METADATA_TOKEN_ERROR_RETRY_SECS period when a fresh token is expected
                            deadline = inner.token.read().unwrap().refresh_in;
                        }
                        Err(error) => {
                            error!(
                                message = "Failed to update GCP authentication token.",
                                %error
                            );
                            deadline = Duration::from_secs(METADATA_TOKEN_ERROR_RETRY_SECS);
                        }
                    }
                }
            }
            Self::ApiKey(_) | Self::None => {
                // This keeps the sender end of the watch open without
                // actually sending anything, effectively creating an
                // empty watch stream.
                sender.closed().await;
            }
        }
    }
}

impl InnerCreds {
    /// Fetches the first token up front so that invalid credentials fail the component build.
    async fn authenticator(source: TokenSource) -> crate::Result<GcpAuthenticator> {
        let token = RwLock::new(source.fetch().await?);
        Ok(GcpAuthenticator::Credentials(Arc::new(Self {
            source,
            token,
        })))
    }

    /// Returns whether the token changed.
    async fn regenerate_token(&self) -> crate::Result<bool> {
        let token = self.source.fetch().await?;
        let mut current = self.token.write().unwrap();
        let changed = current.header != token.header;
        *current = token;
        Ok(changed)
    }

    fn make_token(&self) -> String {
        self.token.read().unwrap().header.clone()
    }
}

impl TokenSource {
    async fn fetch(&self) -> crate::Result<FetchedToken> {
        Ok(match self {
            Self::ServiceAccount(creds, scope) => fetch_token(creds, scope).await?.into(),
            Self::Metadata => get_token_implicit().await?.into(),
            Self::ExternalAccount(account) => account.fetch().await?,
        })
    }
}

impl ExternalAccount {
    fn new(json: serde_json::Value, scope: String) -> crate::Result<Self> {
        let creds = RwLock::new(Self::build(&json, &scope)?);
        Ok(Self { json, scope, creds })
    }

    fn build(json: &serde_json::Value, scope: &str) -> crate::Result<AccessTokenCredentials> {
        Ok(external_account::Builder::new(json.clone())
            .with_scopes([scope])
            .build_access_token_credentials()
            .context(BuildExternalAccountSnafu)?)
    }

    async fn fetch(&self) -> crate::Result<FetchedToken> {
        let creds = self.creds.read().unwrap().clone();
        let token = match creds.access_token().await {
            Ok(token) => token,
            Err(error) => {
                // After a permanent error the SDK stops refreshing and keeps returning that error,
                // so start over with fresh credentials to let the next attempt recover.
                if !error.is_transient() {
                    *self.creds.write().unwrap() = Self::build(&self.json, &self.scope)?;
                }
                return Err(GcpError::GetExternalAccountToken {
                    reason: token_error_reason(&error),
                }
                .into());
            }
        };
        Ok(FetchedToken {
            header: format!("Bearer {}", token.token),
            // ponytail: google-cloud-auth refreshes its cached token 4 minutes before
            // expiry but doesn't expose the expiry, so poll its cache more often than that.
            refresh_in: EXTERNAL_ACCOUNT_REFRESH_INTERVAL,
        })
    }
}

impl From<Token> for FetchedToken {
    fn from(token: Token) -> Self {
        Self {
            header: format!("{} {}", token.token_type(), token.access_token()),
            refresh_in: Duration::from_secs(
                u64::from(token.expires_in())
                    .saturating_sub(METADATA_TOKEN_EXPIRY_MARGIN_SECS)
                    .max(METADATA_TOKEN_ERROR_RETRY_SECS),
            ),
        }
    }
}

/// Describes a token error without the SDK's messages, which can contain credential payloads,
/// signed URLs or credential helper output.
fn token_error_reason(error: &CredentialsError) -> String {
    let status = std::iter::successors(Some(error as &dyn std::error::Error), |error| {
        error.source()
    })
    .find_map(|error| error.downcast_ref::<reqwest::Error>()?.status());
    let hint = if error.is_transient() {
        "will retry"
    } else {
        "check the credentials configuration"
    };
    match status {
        Some(status) => format!("HTTP {status}; {hint}"),
        None => hint.to_owned(),
    }
}

/// Returns the parsed credentials file if it holds Workload Identity Federation
/// (`external_account`) credentials. Anything else is left to `goauth`, which reports its own errors.
fn external_account_json(path: &str) -> Option<serde_json::Value> {
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    (json["type"] == "external_account").then_some(json)
}

async fn fetch_token(creds: &Credentials, scope: &Scope) -> crate::Result<Token> {
    let claims = JwtClaims::new(
        creds.iss(),
        std::slice::from_ref(scope),
        creds.token_uri(),
        None,
        None,
    );
    let rsa_key = creds.rsa_key().context(InvalidRsaKeySnafu)?;
    let jwt = Jwt::new(claims, rsa_key, None);

    debug!(
        message = "Fetching GCP authentication token.",
        project = ?creds.project(),
        iss = ?creds.iss(),
        token_uri = ?creds.token_uri(),
    );
    goauth::get_token(&jwt, creds)
        .await
        .context(GetTokenSnafu)
        .map_err(Into::into)
}

async fn get_token_implicit() -> Result<Token, GcpError> {
    debug!("Fetching implicit GCP authentication token.");
    let req = http::Request::get(SERVICE_ACCOUNT_TOKEN_URL)
        .header("Metadata-Flavor", "Google")
        .body(empty_body())
        .unwrap();

    let proxy = ProxyConfig::from_env();
    let res = HttpClient::new(MaybeTlsSettings::Raw(()), &proxy)
        .context(BuildHttpClientSnafu)?
        .send(req)
        .await
        .context(GetImplicitTokenSnafu)?;

    let body = res.into_body();
    let bytes = body
        .collect()
        .await
        .map(Collected::to_bytes)
        .context(GetTokenBytesSnafu)?;

    // Token::from_str is irresponsible and may panic!
    match serde_json::from_slice::<Token>(&bytes) {
        Ok(token) => Ok(token),
        Err(error) => Err(match serde_json::from_slice::<TokenErr>(&bytes) {
            Ok(error) => GcpError::TokenFromJson { source: error },
            Err(_) => GcpError::TokenJsonFromStr { source: error },
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use tempfile::NamedTempFile;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_string_contains, header, method, path},
    };

    use super::*;
    use crate::assert_downcast_matches;

    #[tokio::test]
    async fn fails_missing_creds() {
        let error = build_auth("").await.expect_err("build failed to error");
        assert_downcast_matches!(error, GcpError, GcpError::GetImplicitToken { .. });
        // This should be a more relevant error
    }

    #[tokio::test]
    async fn skip_authentication() {
        let auth = build_auth(indoc::indoc! {r#"
            skip_authentication: true
            api_key: "testing"
        "#})
        .await
        .expect("build_auth failed");
        assert!(matches!(auth, GcpAuthenticator::None));
    }

    #[tokio::test]
    async fn uses_api_key() {
        let key = crate::test_util::random_string(16);

        let auth = build_auth(&format!("api_key: \"{key}\""))
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
        let error = build_auth(r#"api_key: "abc%xyz""#)
            .await
            .expect_err("build failed to error");
        assert_downcast_matches!(error, GcpError, GcpError::InvalidApiKey { .. });
    }

    #[tokio::test]
    async fn external_account_sets_bearer_token() {
        let sts = MockServer::start().await;
        mock_sts(
            &sts,
            ResponseTemplate::new(200).set_body_json(sts_token("sts-token")),
        )
        .await;
        let (_token_file, creds_file) = external_account_creds(&sts, None);

        let auth = build_auth_from_file(&creds_file)
            .await
            .expect("build_auth failed");

        assert_eq!(authorization(&auth), "Bearer sts-token");
    }

    #[tokio::test]
    async fn external_account_impersonates_service_account() {
        let server = MockServer::start().await;
        mock_sts(
            &server,
            ResponseTemplate::new(200).set_body_json(sts_token("sts-token")),
        )
        .await;
        Mock::given(method("POST"))
            .and(path(IMPERSONATION_PATH))
            .and(header("authorization", "Bearer sts-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accessToken": "sa-token",
                "expireTime": "2099-01-01T00:00:00Z",
            })))
            .mount(&server)
            .await;
        let impersonation_url = format!("{}{IMPERSONATION_PATH}", server.uri());
        let (_token_file, creds_file) = external_account_creds(&server, Some(&impersonation_url));

        let auth = build_auth_from_file(&creds_file)
            .await
            .expect("build_auth failed");

        assert_eq!(authorization(&auth), "Bearer sa-token");
    }

    #[tokio::test]
    async fn external_account_rejected_fails_build() {
        let sts = MockServer::start().await;
        mock_sts(
            &sts,
            ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid_target",
                "error_description": "The target service indicated by the \"audience\" parameters is invalid.",
            })),
        )
        .await;
        let (_token_file, creds_file) = external_account_creds(&sts, None);

        let error = build_auth_from_file(&creds_file)
            .await
            .expect_err("build should fail when STS rejects the subject token");
        assert!(
            error.to_string().contains("HTTP 400 Bad Request"),
            "error should include the STS status: {error}"
        );
        let requests = sts.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "build should fail on the STS exchange");
    }

    #[tokio::test]
    async fn external_account_refresh_without_change_does_not_notify() {
        let sts = MockServer::start().await;
        mock_sts(
            &sts,
            ResponseTemplate::new(200).set_body_json(sts_token("sts-token")),
        )
        .await;
        let (_token_file, creds_file) = external_account_creds(&sts, None);
        let auth = build_auth_from_file(&creds_file)
            .await
            .expect("build_auth failed");
        let GcpAuthenticator::Credentials(inner) = &auth else {
            panic!("expected token credentials");
        };

        // The SDK still holds a valid cached token, so a refresh must not wake subscribers.
        let changed = inner.regenerate_token().await.expect("refresh failed");

        assert!(!changed);
        assert_eq!(authorization(&auth), "Bearer sts-token");
        assert_eq!(sts.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn external_account_recovers_after_permanent_error() {
        let sts = MockServer::start().await;
        // The first exchange is rejected (e.g. an expired subject token), later ones succeed.
        Mock::given(method("POST"))
            .and(path("/v1/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid_grant",
            })))
            .up_to_n_times(1)
            .mount(&sts)
            .await;
        mock_sts(
            &sts,
            ResponseTemplate::new(200).set_body_json(sts_token("sts-token")),
        )
        .await;
        let (_token_file, creds_file) = external_account_creds(&sts, None);
        let json = external_account_json(creds_file.path().to_str().unwrap()).unwrap();
        let account = ExternalAccount::new(json, Scope::Compute.url()).unwrap();

        account
            .fetch()
            .await
            .expect_err("first exchange is rejected");
        let token = account
            .fetch()
            .await
            .expect("should recover once STS accepts the subject token");

        assert_eq!(token.header, "Bearer sts-token");
    }

    #[tokio::test]
    async fn external_account_errors_do_not_leak_subject_token() {
        let mut token_file = NamedTempFile::new().unwrap();
        write!(token_file, r#"{{"id_token":"{SUBJECT_TOKEN}"}}"#).unwrap();
        let creds = serde_json::json!({
            "type": "external_account",
            "audience": "//iam.googleapis.com/projects/1/locations/global/workloadIdentityPools/pool/providers/provider",
            "subject_token_type": "urn:ietf:params:oauth:token-type:jwt",
            "token_url": "http://127.0.0.1:1/v1/token",
            "credential_source": {
                "file": token_file.path().to_str().unwrap(),
                // Typo: the SDK can't find the field and reports the file contents.
                "format": { "type": "json", "subject_token_field_name": "id_token_typo" },
            },
        });
        let mut creds_file = NamedTempFile::new().unwrap();
        write!(creds_file, "{creds}").unwrap();

        let error = build_auth_from_file(&creds_file)
            .await
            .expect_err("build should fail to read the subject token");

        let message = error.to_string();
        assert!(!message.contains(SUBJECT_TOKEN), "{message}");
    }

    #[tokio::test]
    async fn external_account_errors_do_not_leak_credential_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        // A URL-sourced subject token, e.g. from a signed URL.
        let creds = serde_json::json!({
            "type": "external_account",
            "audience": "//iam.googleapis.com/projects/1/locations/global/workloadIdentityPools/pool/providers/provider",
            "subject_token_type": "urn:ietf:params:oauth:token-type:jwt",
            "token_url": format!("{}/v1/token", server.uri()),
            "credential_source": {
                "url": format!("{}/subject-token?signature=SECRET_SIGNATURE", server.uri()),
            },
        });
        let mut creds_file = NamedTempFile::new().unwrap();
        write!(creds_file, "{creds}").unwrap();

        let error = build_auth_from_file(&creds_file)
            .await
            .expect_err("build should fail to fetch the subject token");

        let message = error.to_string();
        assert!(message.contains("HTTP 403 Forbidden"), "{message}");
        assert!(!message.contains("SECRET_SIGNATURE"), "{message}");
    }

    const SUBJECT_TOKEN: &str = "oidc-subject-token";
    const IMPERSONATION_PATH: &str =
        "/v1/projects/-/serviceAccounts/sa@test.iam.gserviceaccount.com:generateAccessToken";

    async fn mock_sts(server: &MockServer, response: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path("/v1/token"))
            .and(body_string_contains(format!(
                "subject_token={SUBJECT_TOKEN}"
            )))
            .respond_with(response)
            .mount(server)
            .await;
    }

    fn sts_token(token: &str) -> serde_json::Value {
        serde_json::json!({
            "access_token": token,
            "issued_token_type": "urn:ietf:params:oauth:token-type:access_token",
            "token_type": "Bearer",
            "expires_in": 3600,
        })
    }

    /// Writes an OIDC subject token file and an `external_account` credentials file pointing at it.
    fn external_account_creds(
        sts: &MockServer,
        impersonation_url: Option<&str>,
    ) -> (NamedTempFile, NamedTempFile) {
        let mut token_file = NamedTempFile::new().unwrap();
        write!(token_file, "{SUBJECT_TOKEN}").unwrap();

        let mut creds = serde_json::json!({
            "type": "external_account",
            "audience": "//iam.googleapis.com/projects/1/locations/global/workloadIdentityPools/pool/providers/provider",
            "subject_token_type": "urn:ietf:params:oauth:token-type:jwt",
            "token_url": format!("{}/v1/token", sts.uri()),
            "credential_source": {
                "file": token_file.path().to_str().unwrap(),
                "format": { "type": "text" },
            },
        });
        if let Some(url) = impersonation_url {
            creds["service_account_impersonation_url"] = url.into();
        }
        let mut creds_file = NamedTempFile::new().unwrap();
        write!(creds_file, "{creds}").unwrap();
        (token_file, creds_file)
    }

    async fn build_auth_from_file(file: &NamedTempFile) -> crate::Result<GcpAuthenticator> {
        // Normalize to forward slashes so YAML doesn't interpret Windows path separators as escapes.
        let path = file.path().to_str().unwrap().replace('\\', "/");
        build_auth(&format!("credentials_path: \"{path}\"")).await
    }

    fn authorization(auth: &GcpAuthenticator) -> String {
        let mut request = http::Request::new(());
        auth.apply(&mut request);
        request.headers()[AUTHORIZATION]
            .to_str()
            .unwrap()
            .to_owned()
    }

    fn apply_uri(auth: &GcpAuthenticator, uri: &str) -> String {
        let mut uri: Uri = uri.parse().unwrap();
        auth.apply_uri(&mut uri);
        uri.to_string()
    }

    async fn build_auth(yaml: &str) -> crate::Result<GcpAuthenticator> {
        let config: GcpAuthConfig = serde_yaml::from_str(yaml).expect("Invalid YAML");
        config.build(Scope::Compute).await
    }
}
