use futures_util::FutureExt;
use http::{Request, StatusCode, Uri};
use hyper::body::Body;
use snafu::Snafu;
use vector_lib::{
    config::AcknowledgementsConfig, configurable::configurable_component,
    sensitive_string::SensitiveString, tls::TlsEnableableConfig,
};

use crate::{
    common::datadog::{self, get_api_base_endpoint},
    http::{HttpClient, HttpError},
    sinks::HealthcheckError,
};

use super::Healthcheck;

#[cfg(feature = "sinks-datadog_events")]
pub mod events;
#[cfg(feature = "sinks-datadog_logs")]
pub mod logs;
#[cfg(feature = "sinks-datadog_metrics")]
pub mod metrics;
#[cfg(any(
    all(feature = "sinks-datadog_logs", test),
    all(feature = "sinks-datadog_metrics", test)
))]
mod test_utils;
#[cfg(feature = "sinks-datadog_traces")]
pub mod traces;

/// Shared configuration for Datadog sinks.
/// Contains the maximum set of common settings that applies to all DD sink components.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct LocalDatadogCommonConfig {
    /// The endpoint to send observability data to.
    ///
    /// The endpoint must contain an HTTP scheme, and may specify a hostname or IP
    /// address and port. The API path should NOT be specified as this is handled by
    /// the sink.
    ///
    /// If set, overrides the `site` option.
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::examples = "http://127.0.0.1:8080"))]
    #[configurable(metadata(docs::examples = "http://example.com:12345"))]
    #[serde(default)]
    endpoint: Option<String>,

    /// The Datadog [site][dd_site] to send observability data to.
    ///
    /// This value can also be set by specifying the `DD_SITE` environment variable.
    /// The value specified here takes precedence over the environment variable.
    ///
    /// If not specified by the environment variable, a default value of
    /// `datadoghq.com` is taken.
    ///
    /// [dd_site]: https://docs.datadoghq.com/getting_started/site
    #[configurable(metadata(docs::examples = "us3.datadoghq.com"))]
    #[configurable(metadata(docs::examples = "datadoghq.eu"))]
    site: Option<String>,

    /// The default Datadog [API key][api_key] to use in authentication of HTTP requests.
    ///
    /// If an event has a Datadog [API key][api_key] set explicitly in its metadata, it takes
    /// precedence over this setting.
    ///
    /// This value can also be set by specifying the `DD_API_KEY` environment variable.
    /// The value specified here takes precedence over the environment variable.
    ///
    /// [api_key]: https://docs.datadoghq.com/api/?lang=bash#authentication
    /// [global_options]: /docs/reference/configuration/global-options/#datadog
    #[configurable(metadata(docs::examples = "${DATADOG_API_KEY_ENV_VAR}"))]
    #[configurable(metadata(docs::examples = "ef8d5de700e7989468166c40fc8a0ccd"))]
    default_api_key: Option<SensitiveString>,

    #[configurable(derived)]
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

impl LocalDatadogCommonConfig {
    pub fn new(
        endpoint: Option<String>,
        site: Option<String>,
        default_api_key: Option<SensitiveString>,
    ) -> Self {
        Self {
            endpoint,
            site,
            default_api_key,
            ..Default::default()
        }
    }

    pub fn with_globals(
        &self,
        config: datadog::Options,
    ) -> Result<DatadogCommonConfig, ConfigurationError> {
        Ok(DatadogCommonConfig {
            endpoint: self.endpoint.clone(),
            site: self.site.clone().unwrap_or(config.site),
            default_api_key: self
                .default_api_key
                .clone()
                .or(config.api_key)
                .ok_or(ConfigurationError::ApiKeyRequired)?,
            acknowledgements: self.acknowledgements,
        })
    }
}

#[derive(Debug, Snafu, PartialEq, Eq)]
pub enum ConfigurationError {
    #[snafu(display("API Key must be specified."))]
    ApiKeyRequired,
}

#[derive(Clone, Debug, Default)]
pub struct DatadogCommonConfig {
    pub endpoint: Option<String>,
    pub site: String,
    pub default_api_key: SensitiveString,
    pub acknowledgements: AcknowledgementsConfig,
}

impl DatadogCommonConfig {
    /// Returns a `Healthcheck` which is a future that will be used to ensure the
    /// `<site>/api/v1/validate` endpoint is reachable.
    fn build_healthcheck(&self, client: HttpClient) -> crate::Result<Healthcheck> {
        let validate_endpoint =
            get_api_validate_endpoint(self.endpoint.as_ref(), self.site.as_str())?;

        let api_key: String = self.default_api_key.clone().into();

        Ok(build_healthcheck_future(client, validate_endpoint, api_key).boxed())
    }
}

/// Makes a GET HTTP request to `<site>/api/v1/validate` using the provided client and API key.
async fn build_healthcheck_future(
    client: HttpClient,
    validate_endpoint: Uri,
    api_key: String,
) -> crate::Result<()> {
    let request = Request::get(validate_endpoint)
        .header("DD-API-KEY", api_key)
        .body(hyper::Body::empty())
        .unwrap();

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        other => Err(HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}

/// Gets the API endpoint for validating credentials.
///
/// If `endpoint` is not specified, we fallback to `site`.
fn get_api_validate_endpoint(endpoint: Option<&String>, site: &str) -> crate::Result<Uri> {
    let base = get_api_base_endpoint(endpoint, site);
    let validate = format!("{}{}", base, "/api/v1/validate");
    validate.parse::<Uri>().map_err(Into::into)
}

#[derive(Debug, Snafu)]
pub enum DatadogApiError {
    #[snafu(display("Failed to make HTTP(S) request: {}", error))]
    HttpError { error: HttpError },
    #[snafu(display("Client request was not valid for unknown reasons."))]
    BadRequest,
    #[snafu(display("Client request was unauthorized."))]
    Unauthorized,
    #[snafu(display("Client request was forbidden."))]
    Forbidden,
    #[snafu(display("Client request timed out."))]
    RequestTimeout,
    #[snafu(display("Client sent a payload that is too large."))]
    PayloadTooLarge,
    #[snafu(display("Client sent too many requests (rate limiting)."))]
    TooManyRequests,
    #[snafu(display("Client request was invalid."))]
    ClientError,
    #[snafu(display("Server responded with an error."))]
    ServerError,
}

impl DatadogApiError {
    /// Common DatadogApiError handling for HTTP Responses.
    /// Returns Ok(response) if the response was Ok/Accepted.
    pub fn from_result(
        result: Result<http::Response<Body>, HttpError>,
    ) -> Result<http::Response<Body>, DatadogApiError> {
        match result {
            Ok(response) => {
                match response.status() {
                    // From https://docs.datadoghq.com/api/latest/logs/:
                    //
                    // The status codes answered by the HTTP API are:
                    // 200: OK (v1)
                    // 202: Accepted (v2)
                    // 400: Bad request (likely an issue in the payload
                    //      formatting)
                    // 401: Unauthorized (likely a missing API Key))
                    // 403: Permission issue (likely using an invalid API Key)
                    // 408: Request Timeout, request should be retried after some
                    // 413: Payload too large (batch is above 5MB uncompressed)
                    // 429: Too Many Requests, request should be retried after some time
                    // 500: Internal Server Error, the server encountered an unexpected condition
                    //      that prevented it from fulfilling the request, request should be
                    //      retried after some time
                    // 503: Service Unavailable, the server is not ready to handle the request
                    //      probably because it is overloaded, request should be retried after some time
                    s if s.is_success() => Ok(response),
                    StatusCode::BAD_REQUEST => Err(DatadogApiError::BadRequest),
                    StatusCode::UNAUTHORIZED => Err(DatadogApiError::Unauthorized),
                    StatusCode::FORBIDDEN => Err(DatadogApiError::Forbidden),
                    StatusCode::REQUEST_TIMEOUT => Err(DatadogApiError::RequestTimeout),
                    StatusCode::PAYLOAD_TOO_LARGE => Err(DatadogApiError::PayloadTooLarge),
                    StatusCode::TOO_MANY_REQUESTS => Err(DatadogApiError::TooManyRequests),
                    s if s.is_client_error() => Err(DatadogApiError::ClientError),
                    _ => Err(DatadogApiError::ServerError),
                }
            }
            Err(error) => Err(DatadogApiError::HttpError { error }),
        }
    }

    pub const fn is_retriable(&self) -> bool {
        match self {
            // This retry logic will be expanded further, but specifically retrying unauthorized
            // requests and lower level HttpErrors for now.
            // I verified using `curl` that `403` is the respose code for this.
            //
            // https://github.com/vectordotdev/vector/issues/10870
            // https://github.com/vectordotdev/vector/issues/12220
            DatadogApiError::HttpError { error } => error.is_retriable(),
            DatadogApiError::BadRequest | DatadogApiError::PayloadTooLarge => false,
            DatadogApiError::ServerError
            | DatadogApiError::ClientError
            | DatadogApiError::Unauthorized
            | DatadogApiError::Forbidden
            | DatadogApiError::RequestTimeout
            | DatadogApiError::TooManyRequests => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_config_with_no_overrides() {
        let local = LocalDatadogCommonConfig::new(
            None,
            Some("potato.com".into()),
            Some("key".to_string().into()),
        );
        let global = datadog::Options {
            api_key: Some("more key".to_string().into()),
            site: "tomato.com".into(),
        };

        let overriden = local.with_globals(global).unwrap();

        assert_eq!(None, overriden.endpoint);
        assert_eq!("potato.com".to_string(), overriden.site);
        assert_eq!(
            SensitiveString::from("key".to_string()),
            overriden.default_api_key
        );
    }

    #[test]
    fn local_config_with_overrides() {
        let local = LocalDatadogCommonConfig::new(None, None, None);
        let global = datadog::Options {
            api_key: Some("more key".to_string().into()),
            site: "tomato.com".into(),
        };

        let overriden = local.with_globals(global).unwrap();

        assert_eq!(None, overriden.endpoint);
        assert_eq!("tomato.com".to_string(), overriden.site);
        assert_eq!(
            SensitiveString::from("more key".to_string()),
            overriden.default_api_key
        );
    }

    #[test]
    fn no_api_key() {
        let local = LocalDatadogCommonConfig::new(None, None, None);
        let global = datadog::Options {
            api_key: None,
            site: "tomato.com".into(),
        };

        let error = local.with_globals(global).unwrap_err();
        assert_eq!(ConfigurationError::ApiKeyRequired, error);
    }
}
