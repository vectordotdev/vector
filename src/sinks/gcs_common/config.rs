use futures::FutureExt;
use goauth::{auth::TokenErr, GoErr};
use http::{StatusCode, Uri};
use hyper::Body;
use serde::{Deserialize, Serialize};
use snafu::Snafu;

use crate::{
    http::{HttpClient, HttpError},
    sinks::{
        gcp::GcpCredentials,
        gcs_common::service::GcsResponse,
        util::retries::{RetryAction, RetryLogic},
        Healthcheck, HealthcheckError,
    },
    template::TemplateParseError,
};

pub const BASE_URL: &str = "https://storage.googleapis.com/";

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "kebab-case")]
pub enum GcsPredefinedAcl {
    AuthenticatedRead,
    BucketOwnerFullControl,
    BucketOwnerRead,
    Private,
    #[derivative(Default)]
    ProjectPrivate,
    PublicRead,
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GcsStorageClass {
    #[derivative(Default)]
    Standard,
    Nearline,
    Coldline,
    Archive,
}

#[derive(Debug, Snafu)]
pub enum GcsError {
    #[snafu(display("Bucket {:?} not found", bucket))]
    BucketNotFound { bucket: String },
}

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

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GcsHealthcheckError {
    #[snafu(display("Invalid credentials"))]
    InvalidCredentials,
    #[snafu(display("Unknown bucket: {:?}", bucket))]
    UnknownBucket { bucket: String },
    #[snafu(display("key_prefix template parse error: {}", source))]
    KeyPrefixTemplate { source: TemplateParseError },
}

pub fn build_healthcheck(
    bucket: String,
    client: HttpClient,
    base_url: String,
    creds: Option<GcpCredentials>,
) -> crate::Result<Healthcheck> {
    let healthcheck = async move {
        let uri = base_url.parse::<Uri>()?;
        let mut request = http::Request::head(uri).body(Body::empty())?;

        if let Some(creds) = creds.as_ref() {
            creds.apply(&mut request);
        }

        let not_found_error = GcsError::BucketNotFound { bucket }.into();

        let response = client.send(request).await?;
        healthcheck_response(creds, not_found_error)(response)
    };

    Ok(healthcheck.boxed())
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
            if let Some(creds) = creds {
                creds.spawn_regenerate_token();
            }
            Ok(())
        }
        StatusCode::FORBIDDEN => Err(GcpError::InvalidCredentials0.into()),
        StatusCode::NOT_FOUND => Err(not_found_error),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

#[derive(Clone)]
pub struct GcsRetryLogic;

// This is a clone of HttpRetryLogic for the Body type, should get merged
impl RetryLogic for GcsRetryLogic {
    type Error = hyper::Error;
    type Response = GcsResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.inner.status();

        match status {
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(format!("{}", status).into()),
            _ if status.is_success() => RetryAction::Successful,
            _ => RetryAction::DontRetry(format!("response status: {}", status).into()),
        }
    }
}
