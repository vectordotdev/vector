#[cfg(feature = "aws-core")]
use aws_credential_types::provider::SharedCredentialsProvider;
#[cfg(feature = "aws-core")]
use aws_types::region::Region;
use bytes::Bytes;
use http::{HeaderName, HeaderValue, StatusCode, header};

/// Maximum number of response body bytes to include in a non-retriable error's reason.
pub(crate) const MAX_ERROR_BODY_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OrderedHeaderName(HeaderName);

impl OrderedHeaderName {
    pub const fn new(header_name: HeaderName) -> Self {
        Self(header_name)
    }

    pub const fn inner(&self) -> &HeaderName {
        &self.0
    }
}

impl From<HeaderName> for OrderedHeaderName {
    fn from(header_name: HeaderName) -> Self {
        Self(header_name)
    }
}

impl Ord for OrderedHeaderName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_str().cmp(other.0.as_str())
    }
}

impl PartialOrd for OrderedHeaderName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
use std::{collections::BTreeMap, marker::PhantomData};

use snafu::{ResultExt, Snafu};
use vector_http::HttpError;
use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf, configurable::configurable_component,
    stream::batcher::limiter::ItemBatchSize,
};

use super::{
    TowerRequestConfig,
    retries::{RetryAction, RetryLogic},
};
use crate::{event::Event, sinks::prelude::*};

#[cfg(feature = "aws-core")]
#[derive(Clone)]
pub struct SigV4Config {
    pub(crate) shared_credentials_provider: SharedCredentialsProvider,
    pub(crate) region: Region,
    pub(crate) service: String,
}

/// Serializes and deserializes a [`Vec<StatusCode>`]
mod status_code_vec {
    use http::StatusCode;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

    /// Deserializes a [`Vec<StatusCode>`]
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<StatusCode>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<u16>::deserialize(deserializer)?
            .into_iter()
            .map(StatusCode::from_u16)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::custom)
    }

    /// Serializes a [`Vec<StatusCode>`]
    pub fn serialize<S>(status_codes: &[StatusCode], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        status_codes
            .iter()
            .map(StatusCode::as_u16)
            .collect::<Vec<_>>()
            .serialize(serializer)
    }
}

/// Configurable retry strategy for `http` based sinks.
///
/// For more information about error responses, see [Client Error Responses][error_responses].
///
/// [error_responses]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status#client_error_responses
#[configurable_component]
#[derive(Debug, Clone, Default, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The retry strategy enum."))]
pub enum RetryStrategy {
    /// Don't retry any errors, including request timeouts.
    None,

    /// Default strategy. See [`RetryStrategy::retry_action`] for more details.
    #[default]
    Default,

    /// Retry on *all* HTTP status codes except for success codes (2xx)
    All,

    /// Custom retry strategy
    Custom {
        /// Retry on these specific HTTP status codes
        #[serde(with = "status_code_vec")]
        status_codes: Vec<StatusCode>,
    },
}

impl RetryStrategy {
    /// Returns the name of the retry strategy.
    #[must_use]
    const fn name(&self) -> &str {
        match self {
            Self::None => "Never retry strategy",
            Self::Default => "Default retry strategy",
            Self::All => "Retry all strategy",
            Self::Custom { .. } => "Custom retry strategy",
        }
    }

    /// Determines if the given status code should be retried.
    ///
    /// For the `Default` strategy, the following status codes will be retried:
    /// - 429 (Too Many Requests)
    /// - 408 (Request Timeout)
    /// - 5xx (Server Error)
    ///
    /// For the `Custom` strategy, the status codes specified in the `status_codes` field will be retried.
    ///
    /// For the `All` strategy, all non-success status codes will be retried.
    #[must_use]
    pub fn retry_action<Req>(&self, status: StatusCode) -> RetryAction<Req> {
        if status.is_success() {
            return RetryAction::Successful;
        }

        let reason = format!(
            "{}: {}",
            self.name(),
            status.canonical_reason().unwrap_or_else(|| status.as_str())
        )
        .into();

        match self {
            Self::None => RetryAction::DontRetry(reason),
            Self::Default => match status {
                StatusCode::TOO_MANY_REQUESTS | StatusCode::REQUEST_TIMEOUT => {
                    RetryAction::Retry(reason)
                }
                StatusCode::NOT_IMPLEMENTED => RetryAction::DontRetry(reason),
                _ => {
                    if status.is_server_error() {
                        RetryAction::Retry(reason)
                    } else {
                        RetryAction::DontRetry(reason)
                    }
                }
            },
            Self::All => RetryAction::Retry(reason),
            Self::Custom { status_codes } => {
                if status_codes.contains(&status) {
                    RetryAction::Retry(reason)
                } else {
                    RetryAction::DontRetry(reason)
                }
            }
        }
    }
}

/// Retry logic driven by a status code extracted from each response.
#[derive(Debug)]
pub struct HttpStatusRetryLogic<F, Req, Res> {
    func: F,
    request: PhantomData<Req>,
    response: PhantomData<Res>,
    retry_strategy: RetryStrategy,
}

impl<F, Req, Res> HttpStatusRetryLogic<F, Req, Res>
where
    F: Fn(&Res) -> StatusCode + Clone + Send + Sync + 'static,
    Req: Send + Sync + 'static,
    Res: Send + Sync + 'static,
{
    pub const fn new(func: F, retry_strategy: RetryStrategy) -> Self {
        HttpStatusRetryLogic {
            func,
            request: PhantomData,
            response: PhantomData,
            retry_strategy,
        }
    }
}

impl<F, Req, Res> RetryLogic for HttpStatusRetryLogic<F, Req, Res>
where
    F: Fn(&Res) -> StatusCode + Clone + Send + Sync + 'static,
    Req: Send + Sync + 'static,
    Res: Send + Sync + 'static,
{
    type Error = HttpError;
    type Request = Req;
    type Response = Res;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        if self.retry_strategy == RetryStrategy::None {
            false
        } else {
            error.is_retriable()
        }
    }

    fn is_retriable_timeout(&self) -> bool {
        self.retry_strategy != RetryStrategy::None
    }

    fn should_retry_response(&self, response: &Res) -> RetryAction<Req> {
        let status = (self.func)(response);
        self.retry_strategy.retry_action(status)
    }
}

impl<F, Req, Res> Clone for HttpStatusRetryLogic<F, Req, Res>
where
    F: Clone,
{
    fn clone(&self) -> Self {
        Self {
            func: self.func.clone(),
            request: PhantomData,
            response: PhantomData,
            retry_strategy: self.retry_strategy.clone(),
        }
    }
}

/// Outbound HTTP request settings.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct RequestConfig {
    #[serde(flatten)]
    pub tower: TowerRequestConfig,

    /// Additional HTTP headers to add to every HTTP request.
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "An HTTP request header and its value. Both header names and values support templating with event data."
    ))]
    #[configurable(metadata(docs::examples = "headers_examples()"))]
    pub headers: BTreeMap<String, String>,
}

fn headers_examples() -> BTreeMap<String, String> {
    btreemap! {
        "Accept" => "text/plain",
        "X-My-Custom-Header" => "A-Value",
        "X-Event-Level" => "{{level}}",
        "X-Event-Timestamp" => "{{timestamp}}",
    }
}

impl RequestConfig {
    /// Split headers into static (non-dynamic) and template (dynamic) maps.
    #[must_use]
    pub fn split_headers(&self) -> (BTreeMap<String, String>, BTreeMap<String, Template>) {
        let mut static_headers = BTreeMap::new();
        let mut template_headers = BTreeMap::new();

        for (name, value) in &self.headers {
            match Template::try_from(value.as_str()) {
                Ok(template) if !template.is_dynamic() => {
                    static_headers.insert(name.clone(), value.clone());
                }
                Ok(template) => {
                    template_headers.insert(name.clone(), template);
                }
                Err(_) => {
                    static_headers.insert(name.clone(), value.clone());
                }
            }
        }

        (static_headers, template_headers)
    }
}

#[derive(Debug, Snafu)]
pub enum HeaderValidationError {
    #[snafu(display("{}: {}", source, name))]
    InvalidHeaderName {
        name: String,
        source: header::InvalidHeaderName,
    },
    #[snafu(display("{}: {}", source, value))]
    InvalidHeaderValue {
        value: String,
        source: header::InvalidHeaderValue,
    },
}

/// Validates HTTP header names and values.
///
/// # Errors
///
/// Returns an error if a header name or value is invalid.
pub fn validate_headers(
    headers: &BTreeMap<String, String>,
) -> crate::Result<BTreeMap<OrderedHeaderName, HeaderValue>> {
    let mut validated_headers = BTreeMap::new();
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .with_context(|_| InvalidHeaderNameSnafu { name })?;
        let value = HeaderValue::from_bytes(value.as_bytes())
            .with_context(|_| InvalidHeaderValueSnafu { value })?;

        validated_headers.insert(name.into(), value);
    }

    Ok(validated_headers)
}

/// Request type for use in the `Service` implementation of HTTP stream sinks.
#[derive(Debug, Clone)]
pub struct HttpRequest<T: Send> {
    payload: Bytes,
    finalizers: EventFinalizers,
    request_metadata: RequestMetadata,
    additional_metadata: T,
}

impl<T: Send> HttpRequest<T> {
    /// Creates a new `HttpRequest`.
    pub const fn new(
        payload: Bytes,
        finalizers: EventFinalizers,
        request_metadata: RequestMetadata,
        additional_metadata: T,
    ) -> Self {
        Self {
            payload,
            finalizers,
            request_metadata,
            additional_metadata,
        }
    }

    pub const fn get_additional_metadata(&self) -> &T {
        &self.additional_metadata
    }

    pub fn take_payload(&mut self) -> Bytes {
        std::mem::take(&mut self.payload)
    }
}

impl<T: Send> Finalizable for HttpRequest<T> {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl<T: Send> MetaDescriptive for HttpRequest<T> {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

impl<T: Send> ByteSizeOf for HttpRequest<T> {
    fn allocated_bytes(&self) -> usize {
        self.payload.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

/// Uses the estimated json encoded size to determine batch sizing.
#[derive(Default)]
pub struct HttpJsonBatchSizer;

impl ItemBatchSize<Event> for HttpJsonBatchSizer {
    fn size(&self, item: &Event) -> usize {
        item.estimated_json_encoded_size_of().get()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn retry_strategy_none_preserves_success_and_rejects_failures() {
        let strategy = RetryStrategy::None;

        assert!(strategy.retry_action::<()>(StatusCode::OK).is_successful());
        assert!(
            strategy
                .retry_action::<()>(StatusCode::INTERNAL_SERVER_ERROR)
                .is_not_retryable()
        );
    }

    #[test]
    fn retry_strategy_none_disables_timeout_retries() {
        let status_logic =
            HttpStatusRetryLogic::<_, (), ()>::new(|(): &()| StatusCode::OK, RetryStrategy::None);

        assert!(!status_logic.is_retriable_timeout());
    }

    #[test]
    fn retry_strategy_all_preserves_success_and_retries_failures() {
        let strategy = RetryStrategy::All;

        assert!(strategy.retry_action::<()>(StatusCode::OK).is_successful());
        assert!(
            strategy
                .retry_action::<()>(StatusCode::BAD_REQUEST)
                .is_retryable()
        );
        assert!(
            strategy
                .retry_action::<()>(StatusCode::INTERNAL_SERVER_ERROR)
                .is_retryable()
        );
    }

    #[test]
    fn retry_strategy_custom_only_retries_configured_statuses() {
        let strategy = RetryStrategy::Custom {
            status_codes: vec![StatusCode::BAD_REQUEST],
        };

        assert!(strategy.retry_action::<()>(StatusCode::OK).is_successful());
        assert!(
            strategy
                .retry_action::<()>(StatusCode::BAD_REQUEST)
                .is_retryable()
        );
        assert!(
            strategy
                .retry_action::<()>(StatusCode::INTERNAL_SERVER_ERROR)
                .is_not_retryable()
        );
    }

    #[test]
    fn retry_strategy_custom_serde_roundtrips_status_codes() {
        let json = r#"{"type":"custom","status_codes":[400,503]}"#;
        let strategy: RetryStrategy = serde_json::from_str(json).unwrap();
        assert_eq!(
            strategy,
            RetryStrategy::Custom {
                status_codes: vec![StatusCode::BAD_REQUEST, StatusCode::SERVICE_UNAVAILABLE],
            }
        );
        let encoded = serde_json::to_string(&strategy).unwrap();
        let roundtrip: RetryStrategy = serde_json::from_str(&encoded).unwrap();
        assert_eq!(roundtrip, strategy);
    }

    #[test]
    fn retry_strategy_custom_serde_rejects_invalid_status_codes() {
        // `StatusCode::from_u16` only accepts 100–999; 1000 is out of range.
        let json = r#"{"type":"custom","status_codes":[1000]}"#;
        let result = serde_json::from_str::<RetryStrategy>(json);
        assert!(
            result.is_err(),
            "expected invalid status code to fail deserialization"
        );
    }
}
