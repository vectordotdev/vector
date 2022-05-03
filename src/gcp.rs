use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures::StreamExt;
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
    #[snafu(display("Invalid GCP credentials"))]
    InvalidCredentials0,
    #[snafu(display("Invalid GCP credentials"))]
    InvalidCredentials1 { source: GoErr },
    #[snafu(display("Invalid RSA key in GCP credentials"))]
    InvalidRsaKey { source: GoErr },
    #[snafu(display("Failed to get OAuth token"))]
    GetToken { source: GoErr },
    #[snafu(display("Failed to get OAuth token text"))]
    GetTokenBytes { source: hyper::Error },
    #[snafu(display("Failed to get implicit GCP token"))]
    GetImplicitToken { source: HttpError },
    #[snafu(display("Failed to parse OAuth token JSON"))]
    TokenFromJson { source: TokenErr },
    #[snafu(display("Failed to parse OAuth token JSON text"))]
    TokenJsonFromStr { source: serde_json::Error },
    #[snafu(display("Failed to build HTTP client"))]
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
pub struct GcpCredentials {
    creds: Option<Credentials>,
    scope: Scope,
    token: Arc<RwLock<Token>>,
}

impl GcpCredentials {
    async fn from_file(path: &str, scope: Scope) -> crate::Result<Self> {
        let creds = Credentials::from_file(path).context(InvalidCredentials1Snafu)?;
        let jwt = make_jwt(&creds, &scope)?;
        let token = goauth::get_token(&jwt, &creds)
            .await
            .context(GetTokenSnafu)?;
        Ok(Self {
            creds: Some(creds),
            scope,
            token: Arc::new(RwLock::new(token)),
        })
    }

    async fn new_implicit(scope: Scope) -> crate::Result<Self> {
        let token = get_token_implicit().await?;
        Ok(Self {
            creds: None,
            scope,
            token: Arc::new(RwLock::new(token)),
        })
    }

    pub fn make_token(&self) -> String {
        let token = self.token.read().unwrap();
        format!("{} {}", token.token_type(), token.access_token())
    }

    pub fn apply<T>(&self, request: &mut http::Request<T>) {
        request
            .headers_mut()
            .insert(AUTHORIZATION, self.make_token().parse().unwrap());
    }

    async fn regenerate_token(&self) -> crate::Result<()> {
        let token = match &self.creds {
            Some(creds) => {
                let jwt = make_jwt(creds, &self.scope).unwrap(); // Errors caught above
                goauth::get_token(&jwt, creds).await?
            }
            None => get_token_implicit().await?,
        };
        *self.token.write().unwrap() = token;
        Ok(())
    }

    pub fn spawn_regenerate_token(&self) {
        let this = self.clone();

        let period = this.token.read().unwrap().expires_in() as u64 / 2;
        let interval = IntervalStream::new(tokio::time::interval(Duration::from_secs(period)));
        let task = interval.for_each(move |_| {
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
        });
        tokio::spawn(task);
    }
}

async fn get_token_implicit() -> Result<Token, GcpError> {
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

fn make_jwt(creds: &Credentials, scope: &Scope) -> crate::Result<Jwt<JwtClaims>> {
    let claims = JwtClaims::new(creds.iss(), scope, creds.token_uri(), None, None);
    let rsa_key = creds.rsa_key().context(InvalidRsaKeySnafu)?;
    Ok(Jwt::new(claims, rsa_key, None))
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
