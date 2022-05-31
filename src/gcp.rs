use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures::{Stream, StreamExt};
pub use goauth::scopes::Scope;
use goauth::{
    auth::{JwtClaims, Token, TokenErr},
    credentials::Credentials,
    GoErr,
};
use hyper::header::AUTHORIZATION;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use smpl_jwt::Jwt;
use snafu::{ResultExt, Snafu};
use tokio::time::Instant;
use tokio_stream::wrappers::IntervalStream;

use crate::{config::ProxyConfig, http::HttpClient, http::HttpError};

const SERVICE_ACCOUNT_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct GcpAuthConfig {
    pub api_key: Option<String>,
    pub credentials_path: Option<String>,
}

impl GcpAuthConfig {
    pub async fn make_credentials(&self, scope: Scope) -> crate::Result<Option<GcpCredentials>> {
        let gap = std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok();
        let creds_path = self.credentials_path.as_ref().or_else(|| gap.as_ref());
        Ok(match (&creds_path, &self.api_key) {
            (Some(path), _) => Some(GcpCredentials::from_file(path, scope).await?),
            (None, Some(_)) => None,
            (None, None) => Some(GcpCredentials::new_implicit(scope).await?),
        })
    }
}

#[derive(Clone, Debug)]
pub struct GcpCredentials(Arc<Inner>);

#[derive(Debug)]
struct Inner {
    creds: Option<Credentials>,
    scope: Scope,
    token: RwLock<Token>,
}

impl GcpCredentials {
    async fn from_file(path: &str, scope: Scope) -> crate::Result<Self> {
        let creds = Credentials::from_file(path).context(InvalidCredentialsSnafu)?;
        let token = fetch_token(&creds, &scope).await?;
        Ok(Self(Arc::new(Inner {
            creds: Some(creds),
            scope,
            token: RwLock::new(token),
        })))
    }

    async fn new_implicit(scope: Scope) -> crate::Result<Self> {
        let token = get_token_implicit().await?;
        Ok(Self(Arc::new(Inner {
            creds: None,
            scope,
            token: RwLock::new(token),
        })))
    }

    pub fn make_token(&self) -> String {
        let token = self.0.token.read().unwrap();
        format!("{} {}", token.token_type(), token.access_token())
    }

    pub fn apply<T>(&self, request: &mut http::Request<T>) {
        request
            .headers_mut()
            .insert(AUTHORIZATION, self.make_token().parse().unwrap());
    }

    async fn regenerate_token(&self) -> crate::Result<()> {
        let token = match &self.0.creds {
            Some(creds) => fetch_token(creds, &self.0.scope).await?,
            None => get_token_implicit().await?,
        };
        *self.0.token.write().unwrap() = token;
        Ok(())
    }

    pub fn spawn_regenerate_token(&self) {
        let this = self.clone();
        tokio::spawn(async move { this.token_regenerator().for_each(|_| async {}).await });
    }

    pub fn token_regenerator(&self) -> impl Stream<Item = ()> + 'static {
        let period = Duration::from_secs(self.0.token.read().unwrap().expires_in() as u64 / 2);
        let this = self.clone();
        IntervalStream::new(tokio::time::interval_at(Instant::now() + period, period)).then(
            move |_| {
                let this = this.clone();
                async move {
                    debug!("Renewing GCP authentication token.");
                    if let Err(error) = this.regenerate_token().await {
                        error!(
                            message = "Failed to update GCP authentication token.",
                            %error
                        );
                    }
                }
            },
        )
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
    #[ignore]
    async fn fails_missing_creds() {
        let config: GcpAuthConfig = toml::from_str("").unwrap();
        match config.make_credentials(Scope::Compute).await {
            Ok(_) => panic!("make_credentials failed to error"),
            Err(err) => assert_downcast_matches!(err, GcpError, GcpError::GetImplicitToken { .. }), // This should be a more relevant error
        }
    }
}
