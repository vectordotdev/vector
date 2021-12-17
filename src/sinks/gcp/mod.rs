use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use futures::StreamExt;
use goauth::{
    auth::{JwtClaims, Token, TokenErr},
    credentials::Credentials,
    scopes::Scope,
};
use hyper::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use smpl_jwt::Jwt;
use snafu::ResultExt;
use tokio_stream::wrappers::IntervalStream;

use crate::{
    config::ProxyConfig,
    http::HttpClient,
    sinks::gcs_common::config::{
        BuildHttpClient, GcpError, GetImplicitToken, GetToken, GetTokenBytes, InvalidCredentials1,
        InvalidRsaKey,
    },
};

pub mod cloud_storage;
pub mod pubsub;
pub mod stackdriver_logs;
pub mod stackdriver_metrics;

const SERVICE_ACCOUNT_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

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

async fn get_token_implicit() -> Result<Token, GcpError> {
    let req = http::Request::get(SERVICE_ACCOUNT_TOKEN_URL)
        .header("Metadata-Flavor", "Google")
        .body(hyper::Body::empty())
        .unwrap();

    let proxy = ProxyConfig::from_env();
    let res = HttpClient::new(None, &proxy)
        .context(BuildHttpClient)?
        .send(req)
        .await
        .context(GetImplicitToken)?;

    let body = res.into_body();
    let bytes = hyper::body::to_bytes(body).await.context(GetTokenBytes)?;

    // Token::from_str is irresponsible and may panic!
    match serde_json::from_slice::<Token>(&bytes) {
        Ok(token) => Ok(token),
        Err(error) => Err(match serde_json::from_slice::<TokenErr>(&bytes) {
            Ok(error) => GcpError::TokenFromJson { source: error },
            Err(_) => GcpError::TokenJsonFromStr { source: error },
        }),
    }
}

impl GcpCredentials {
    async fn from_file(path: &str, scope: Scope) -> crate::Result<Self> {
        let creds = Credentials::from_file(path).context(InvalidCredentials1)?;
        let jwt = make_jwt(&creds, &scope)?;
        let token = goauth::get_token(&jwt, &creds).await.context(GetToken)?;
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

    pub fn apply<T>(&self, request: &mut http::Request<T>) {
        let token = self.token.read().unwrap();
        let value = format!("{} {}", token.token_type(), token.access_token());
        request
            .headers_mut()
            .insert(AUTHORIZATION, value.parse().unwrap());
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

fn make_jwt(creds: &Credentials, scope: &Scope) -> crate::Result<Jwt<JwtClaims>> {
    let claims = JwtClaims::new(creds.iss(), scope, creds.token_uri(), None, None);
    let rsa_key = creds.rsa_key().context(InvalidRsaKey)?;
    Ok(Jwt::new(claims, rsa_key, None))
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct GcpTypedResource {
    pub r#type: String,
    pub labels: std::collections::HashMap<String, String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "UPPERCASE")]
pub enum GcpMetricKind {
    Cumulative,
    Gauge,
}

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "UPPERCASE")]
pub enum GcpValueType {
    Int64,
}

#[derive(Serialize, Debug, Clone, Copy)]
pub struct GcpPoint {
    pub interval: GcpInterval,
    pub value: GcpPointValue,
}

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct GcpInterval {
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_optional_datetime"
    )]
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(serialize_with = "serialize_datetime")]
    pub end_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct GcpPointValue {
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_int64_value"
    )]
    pub int64_value: Option<i64>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GcpSerie<'a> {
    pub metric: GcpTypedResource,
    pub resource: GcpTypedResource,
    pub metric_kind: GcpMetricKind,
    pub value_type: GcpValueType,
    pub points: &'a [GcpPoint],
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GcpSeries<'a> {
    time_series: &'a [GcpSerie<'a>],
}

fn serialize_int64_value<S>(value: &Option<i64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(value.as_ref().expect("always defined").to_string().as_str())
}

fn serialize_datetime<S>(
    value: &chrono::DateTime<chrono::Utc>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(
        value
            .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
            .as_str(),
    )
}

fn serialize_optional_datetime<S>(
    value: &Option<chrono::DateTime<chrono::Utc>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serialize_datetime(value.as_ref().expect("always defined"), serializer)
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
