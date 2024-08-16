#![allow(missing_docs)]
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use base64::prelude::{Engine as _, BASE64_URL_SAFE};
pub use goauth::scopes::Scope;
use goauth::{
    auth::{JwtClaims, Token, TokenErr},
    credentials::Credentials,
    GoErr,
};
use http::{uri::PathAndQuery, Uri};
use hyper::header::AUTHORIZATION;
use once_cell::sync::Lazy;
use smpl_jwt::Jwt;
use snafu::{ResultExt, Snafu};
use tokio::sync::watch;
use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;

use crate::{config::ProxyConfig, http::HttpClient, http::HttpError};

const SERVICE_ACCOUNT_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

// See https://cloud.google.com/compute/docs/access/authenticate-workloads#applications
const METADATA_TOKEN_EXPIRY_MARGIN_SECS: u64 = 200;

const METADATA_TOKEN_ERROR_RETRY_SECS: u64 = 2;

pub const PUBSUB_URL: &str = "https://pubsub.googleapis.com";

pub static PUBSUB_ADDRESS: Lazy<String> = Lazy::new(|| {
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
    BuildHttpClient { source: HttpError },
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

    /// Path to a [service account][gcp_service_account_credentials] credentials JSON file.
    ///
    /// Either an API key or a path to a service account credentials JSON file can be specified.
    ///
    /// If both are unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
    /// filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
    /// running on. If this is not on a GCE instance, then you must define it with an API key or service account
    /// credentials JSON file.
    ///
    /// [gcp_service_account_credentials]: https://cloud.google.com/docs/authentication/production#manually
    pub credentials_path: Option<String>,

    /// Skip all authentication handling. For use with integration tests only.
    #[serde(default, skip_serializing)]
    #[configurable(metadata(docs::hidden))]
    pub skip_authentication: bool,
}

impl GcpAuthConfig {
    pub async fn build(&self, scope: Scope) -> crate::Result<GcpAuthenticator> {
        Ok(if self.skip_authentication {
            GcpAuthenticator::None
        } else {
            let gap = std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok();
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
    creds: Option<(Credentials, Scope)>,
    token: RwLock<Token>,
}

impl GcpAuthenticator {
    async fn from_file(path: &str, scope: Scope) -> crate::Result<Self> {
        let creds = Credentials::from_file(path).context(InvalidCredentialsSnafu)?;
        let token = RwLock::new(fetch_token(&creds, &scope).await?);
        let creds = Some((creds, scope));
        Ok(Self::Credentials(Arc::new(InnerCreds { creds, token })))
    }

    async fn new_implicit() -> crate::Result<Self> {
        let token = RwLock::new(get_token_implicit().await?);
        let creds = None;
        Ok(Self::Credentials(Arc::new(InnerCreds { creds, token })))
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
                let expires_in = inner.token.read().unwrap().expires_in() as u64;
                let mut deadline =
                    Duration::from_secs(expires_in - METADATA_TOKEN_EXPIRY_MARGIN_SECS);
                loop {
                    tokio::time::sleep(deadline).await;
                    debug!("Renewing GCP authentication token.");
                    match inner.regenerate_token().await {
                        Ok(()) => {
                            sender.send_replace(());
                            let expires_in = inner.token.read().unwrap().expires_in() as u64;
                            deadline =
                                Duration::from_secs(expires_in - METADATA_TOKEN_EXPIRY_MARGIN_SECS);
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
                sender.closed().await
            }
        }
    }
}

impl InnerCreds {
    async fn regenerate_token(&self) -> crate::Result<()> {
        let token = match &self.creds {
            Some((creds, scope)) => fetch_token(creds, scope).await?,
            None => get_token_implicit().await?,
        };
        *self.token.write().unwrap() = token;
        Ok(())
    }

    fn make_token(&self) -> String {
        let token = self.token.read().unwrap();
        format!("{} {}", token.token_type(), token.access_token())
    }
}

async fn fetch_token(creds: &Credentials, scope: &Scope) -> crate::Result<Token> {
    let claims = JwtClaims::new(creds.iss(), scope, creds.token_uri(), None, None);
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
        .body(hyper::Body::empty())
        .unwrap();

    let proxy = ProxyConfig::from_env();
    let res = HttpClient::new(None, &proxy)
        .context(BuildHttpClientSnafu)?
        .send(req)
        .await
        .context(GetImplicitTokenSnafu)?;

    let body = res.into_body();
    let bytes = hyper::body::to_bytes(body)
        .await
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
        config.build(Scope::Compute).await
    }
}
