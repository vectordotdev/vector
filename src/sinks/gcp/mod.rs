use crate::sinks::HealthcheckError;
use futures01::{Future, Stream};
use goauth::scopes::Scope;
use goauth::{
    auth::{JwtClaims, Token, TokenErr},
    credentials::Credentials,
    GoErr,
};
use hyper::{header::AUTHORIZATION, StatusCode};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use smpl_jwt::Jwt;
use snafu::{ResultExt, Snafu};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio01::timer::Interval;

pub mod cloud_storage;
pub mod pubsub;
pub mod stackdriver_logs;

const SERVICE_ACCOUNT_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

#[derive(Debug, Snafu)]
enum GcpError {
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
    GetTokenText { source: reqwest::Error },
    #[snafu(display("Failed to get implicit GCP token"))]
    GetImplicitToken { source: reqwest::Error },
    #[snafu(display("Failed to parse OAuth token JSON"))]
    TokenFromJson { source: TokenErr },
    #[snafu(display("Failed to parse OAuth token JSON text"))]
    TokenJsonFromStr { source: serde_json::Error },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct GcpAuthConfig {
    pub api_key: Option<String>,
    pub credentials_path: Option<String>,
}

impl GcpAuthConfig {
    pub fn make_credentials(&self, scope: Scope) -> crate::Result<Option<GcpCredentials>> {
        let gap = std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok();
        let creds_path = self.credentials_path.as_ref().or(gap.as_ref());
        Ok(match (&creds_path, &self.api_key) {
            (Some(path), _) => Some(GcpCredentials::from_file(path, scope)?),
            (None, Some(_)) => None,
            (None, None) => Some(GcpCredentials::new_implicit(scope)?),
        })
    }
}

#[derive(Clone, Debug)]
pub struct GcpCredentials {
    creds: Option<Credentials>,
    scope: Scope,
    token: Arc<RwLock<Token>>,
}

fn get_token_implicit() -> Result<Token, GcpError> {
    let client = Client::new();
    let mut response = client
        .get(SERVICE_ACCOUNT_TOKEN_URL)
        .header("Metadata-Flavor", "Google")
        .send()
        .context(GetImplicitToken)?;
    let text = response.text().context(GetTokenText)?;
    // Token::from_str is irresponsible and may panic!
    match serde_json::from_str::<Token>(&text) {
        Ok(token) => Ok(token),
        Err(error) => Err(match serde_json::from_str::<TokenErr>(&text) {
            Ok(error) => GcpError::TokenFromJson { source: error },
            Err(_) => GcpError::TokenJsonFromStr { source: error },
        }),
    }
}

fn get_token_with_creds_blocking(
    jwt: &Jwt<JwtClaims>,
    credentials: &Credentials,
) -> Result<Token, GoErr> {
    let mut rt = tokio_compat::runtime::current_thread::Runtime::new()?;
    rt.block_on_std(goauth::get_token(jwt, credentials))
}

impl GcpCredentials {
    fn from_file(path: &str, scope: Scope) -> crate::Result<Self> {
        let creds = Credentials::from_file(path).context(InvalidCredentials1)?;
        let jwt = make_jwt(&creds, &scope)?;
        let token = get_token_with_creds_blocking(&jwt, &creds).context(GetToken)?;
        Ok(Self {
            creds: Some(creds),
            scope,
            token: Arc::new(RwLock::new(token)),
        })
    }

    fn new_implicit(scope: Scope) -> crate::Result<Self> {
        let token = get_token_implicit()?;
        Ok(Self {
            creds: None,
            scope,
            token: Arc::new(RwLock::new(token)),
        })
    }

    pub fn apply<T>(&self, request: &mut http::Request<T>) {
        let token = self.token.read().unwrap();
        let value = format!("{} {}", token.token_type(), token.access_token());
        request
            .headers_mut()
            .insert(AUTHORIZATION, value.parse().unwrap());
    }

    fn regenerate_token(&self) -> crate::Result<()> {
        let token = match &self.creds {
            Some(creds) => {
                let jwt = make_jwt(creds, &self.scope).unwrap(); // Errors caught above
                get_token_with_creds_blocking(&jwt, creds)?
            }
            None => get_token_implicit()?,
        };
        *self.token.write().unwrap() = token;
        Ok(())
    }

    pub fn spawn_regenerate_token(&self) {
        let interval = self.token.read().unwrap().expires_in() as u64 / 2;
        let copy = self.clone();
        let renew_task = Interval::new_interval(Duration::from_secs(interval))
            .for_each(move |_instant| {
                debug!("Renewing GCP authentication token");
                if let Err(error) = copy.regenerate_token() {
                    error!(message = "Failed to update GCP authentication token", %error);
                }
                Ok(())
            })
            .map_err(
                |error| error!(message = "GCP authentication token regenerate interval failed", %error),
            );

        tokio01::spawn(renew_task);
    }
}

fn make_jwt(creds: &Credentials, scope: &Scope) -> crate::Result<Jwt<JwtClaims>> {
    let claims = JwtClaims::new(creds.iss(), scope, creds.token_uri(), None, None);
    let rsa_key = creds.rsa_key().context(InvalidRsaKey)?;
    Ok(Jwt::new(claims, rsa_key, None))
}

// Use this to map a healthcheck response, as it handles setting up the renewal task.
pub fn healthcheck_response(
    creds: Option<GcpCredentials>,
    not_found_error: crate::Error,
) -> impl FnOnce(http::Response<hyper::Body>) -> crate::Result<()> {
    move |response| match response.status() {
        StatusCode::OK => {
            // If there are credentials configured, the
            // generated OAuth token needs to be periodically
            // regenerated. Since the health check runs at
            // startup, after a successful health check is a
            // good place to create the regeneration task.
            creds.map(|creds| creds.spawn_regenerate_token());
            Ok(())
        }
        StatusCode::FORBIDDEN => Err(GcpError::InvalidCredentials0.into()),
        StatusCode::NOT_FOUND => Err(not_found_error),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_downcast_matches;

    #[test]
    #[ignore]
    fn fails_missing_creds() {
        let config: GcpAuthConfig = toml::from_str("").unwrap();
        match config.make_credentials(Scope::Compute) {
            Ok(_) => panic!("make_credentials failed to error"),
            Err(err) => assert_downcast_matches!(err, GcpError, GcpError::GetImplicitToken { .. }), // This should be a more relevant error
        }
    }
}
