use futures_util::FutureExt;
use http::{Request, StatusCode, Uri};
use hyper::body::Body;
use snafu::Snafu;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;
use vector_core::{config::AcknowledgementsConfig, tls::TlsEnableableConfig};

use crate::{
    common::datadog::{get_api_base_endpoint, get_base_domain_region, Region, DD_US_SITE},
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
#[cfg(feature = "sinks-datadog_traces")]
pub mod traces;

/// Get the default Datadog site, which is the US site.
pub(crate) fn default_site() -> String {
    DD_US_SITE.to_owned()
}

/// Shared configuration for Datadog sinks.
/// Contains the maximum set of common settings that applies to all DD sink components.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DatadogCommonConfig {
    /// The endpoint to send observability data to.
    ///
    /// The endpoint must contain an HTTP scheme, and may specify a
    /// hostname or IP address and port.
    ///
    /// If set, overrides the `site` option.
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::examples = "http://127.0.0.1:8080"))]
    #[configurable(metadata(docs::examples = "http://example.com:12345"))]
    #[serde(default)]
    pub endpoint: Option<String>,

    /// The Datadog [site][dd_site] to send observability data to.
    ///
    /// [dd_site]: https://docs.datadoghq.com/getting_started/site
    #[configurable(metadata(docs::examples = "us3.datadoghq.com"))]
    #[configurable(metadata(docs::examples = "datadoghq.eu"))]
    #[serde(default = "default_site")]
    pub site: String,

    /// The default Datadog [API key][api_key] to use in authentication of HTTP requests.
    ///
    /// If an event has a Datadog [API key][api_key] set explicitly in its metadata, it takes
    /// precedence over this setting.
    ///
    /// [api_key]: https://docs.datadoghq.com/api/?lang=bash#authentication
    #[configurable(metadata(docs::examples = "${DATADOG_API_KEY_ENV_VAR}"))]
    #[configurable(metadata(docs::examples = "ef8d5de700e7989468166c40fc8a0ccd"))]
    pub default_api_key: SensitiveString,

    #[configurable(derived)]
    #[serde(default)]
    pub tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl Default for DatadogCommonConfig {
    fn default() -> Self {
        Self {
            endpoint: None,
            site: default_site(),
            default_api_key: SensitiveString::default(),
            tls: None,
            acknowledgements: AcknowledgementsConfig::default(),
        }
    }
}

impl DatadogCommonConfig {
    /// Returns a `Healthcheck` which is a future that will be used to ensure the
    /// `<site>/api/v1/validate` endpoint is reachable.
    fn build_healthcheck(
        &self,
        client: HttpClient,
        region: Option<&Region>,
    ) -> crate::Result<Healthcheck> {
        let validate_endpoint = get_api_validate_endpoint(
            self.endpoint.as_ref(),
            get_base_domain_region(self.site.as_str(), region),
        )?;

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
    #[snafu(display("Server responded with an error."))]
    ServerError,
    #[snafu(display("Failed to make HTTP(S) request: {}", error))]
    HttpError { error: HttpError },
    #[snafu(display("Client sent a payload that is too large."))]
    PayloadTooLarge,
    #[snafu(display("Client request was not valid for unknown reasons."))]
    BadRequest,
    #[snafu(display("Client request was forbidden."))]
    Forbidden,
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
                    // 403: Permission issue (likely using an invalid API Key)
                    // 413: Payload too large (batch is above 5MB uncompressed)
                    // 5xx: Internal error, request should be retried after some
                    //      time
                    StatusCode::BAD_REQUEST => Err(DatadogApiError::BadRequest),
                    StatusCode::FORBIDDEN => Err(DatadogApiError::Forbidden),
                    StatusCode::OK | StatusCode::ACCEPTED => Ok(response),
                    StatusCode::PAYLOAD_TOO_LARGE => Err(DatadogApiError::PayloadTooLarge),
                    _ => Err(DatadogApiError::ServerError),
                }
            }
            Err(error) => Err(DatadogApiError::HttpError { error }),
        }
    }

    pub const fn is_retriable(&self) -> bool {
        match self {
            // This retry logic will be expanded further, but specifically retrying unauthorized
            // requests and lower level HttpErrorsfor now.
            // I verified using `curl` that `403` is the respose code for this.
            //
            // https://github.com/vectordotdev/vector/issues/10870
            // https://github.com/vectordotdev/vector/issues/12220
            DatadogApiError::HttpError { error } => error.is_retriable(),
            DatadogApiError::BadRequest | DatadogApiError::PayloadTooLarge => false,
            DatadogApiError::ServerError | DatadogApiError::Forbidden => true,
        }
    }
}
