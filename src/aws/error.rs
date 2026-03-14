//! Shared AWS error context extraction and classification.
//!
//! Provides zero-allocation structured metadata extraction from any `SdkError`,
//! and high-level error classification for logging and metrics.

use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use aws_types::request_id::RequestId;

/// Known auth error codes returned by AWS services.
const AUTH_CODES: &[&str] = &[
    "AccessDenied",
    "AccessDeniedException",
    "InvalidAccessKeyId",
    "SignatureDoesNotMatch",
    "ExpiredToken",
    "ExpiredTokenException",
    "InvalidClientTokenId",
    "UnrecognizedClientException",
    "IncompleteSignature",
    "MissingAuthenticationToken",
    "InvalidIdentityToken",
];

/// Known not-found error codes returned by AWS services.
const NOT_FOUND_CODES: &[&str] = &[
    "NoSuchKey",
    "NoSuchBucket",
    "NotFound",
    "ResourceNotFoundException",
    "QueueDoesNotExist",
    "AWS.SimpleQueueService.NonExistentQueue",
];

/// Known throttling error codes returned by AWS services.
const THROTTLING_CODES: &[&str] = &[
    "Throttling",
    "ThrottlingException",
    "TooManyRequestsException",
    "RequestExpired",
    "RequestTimeout",
    "ProvisionedThroughputExceededException",
    "SlowDown",
    "RequestLimitExceeded",
];

/// Classification of the SdkError variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwsSdkErrorVariant {
    /// Request failed during construction before being sent.
    ConstructionFailure,
    /// Request failed due to a timeout.
    TimeoutError,
    /// Request failed during dispatch; no HTTP response received.
    DispatchFailure,
    /// Response received but not parseable.
    ResponseError,
    /// Error response received from the service.
    ServiceError,
    /// Unrecognized SdkError variant (future SDK additions).
    Unknown,
}

/// Sub-classification for DispatchFailure errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwsDispatchKind {
    /// The dispatch failed due to a timeout.
    Timeout,
    /// The dispatch failed due to an IO error (DNS, TLS, connection).
    Io,
    /// The dispatch failed due to a user-caused error.
    User,
    /// The dispatch failed for another reason.
    Other,
}

/// High-level error classification for logging and metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwsErrorClass {
    /// 401/403 or known auth error codes (AccessDenied, ExpiredToken, etc.)
    Auth,
    /// 404 or NoSuchKey, NoSuchBucket
    NotFound,
    /// 429 or ThrottlingException, RequestExpired, etc.
    Throttling,
    /// DispatchFailure or TimeoutError — network, DNS, TLS, proxy
    Connectivity,
    /// ConstructionFailure — bad config, credential resolution
    Configuration,
    /// 5xx server errors
    ServiceError,
    /// Other 4xx client errors
    RequestError,
    /// Cannot determine
    Unknown,
}

/// Structured error context extracted from an SdkError. All fields are borrowed.
#[derive(Debug)]
pub struct AwsErrorContext<'a> {
    /// Which SdkError variant produced this context.
    pub variant: AwsSdkErrorVariant,
    /// Sub-classification when variant is DispatchFailure.
    pub dispatch_kind: Option<AwsDispatchKind>,
    /// HTTP status code, if the request reached AWS.
    pub http_status: Option<u16>,
    /// AWS error code from the service response (e.g., "AccessDenied").
    pub aws_error_code: Option<&'a str>,
    /// AWS error message from the service response.
    pub aws_error_message: Option<&'a str>,
    /// AWS request ID, if the request reached AWS.
    pub aws_request_id: Option<&'a str>,
}

/// Extract structured error context from an SdkError.
/// Zero allocations — all returned fields are borrowed from the error.
pub fn extract_error_context<'a, E>(error: &'a SdkError<E, HttpResponse>) -> AwsErrorContext<'a>
where
    E: ProvideErrorMetadata,
{
    match error {
        SdkError::ConstructionFailure(_) => AwsErrorContext {
            variant: AwsSdkErrorVariant::ConstructionFailure,
            dispatch_kind: None,
            http_status: None,
            aws_error_code: None,
            aws_error_message: None,
            aws_request_id: None,
        },
        SdkError::TimeoutError(_) => AwsErrorContext {
            variant: AwsSdkErrorVariant::TimeoutError,
            dispatch_kind: None,
            http_status: None,
            aws_error_code: None,
            aws_error_message: None,
            aws_request_id: None,
        },
        SdkError::DispatchFailure(failure) => {
            let dispatch_kind = if failure.is_timeout() {
                AwsDispatchKind::Timeout
            } else if failure.is_io() {
                AwsDispatchKind::Io
            } else if failure.is_user() {
                AwsDispatchKind::User
            } else {
                AwsDispatchKind::Other
            };
            AwsErrorContext {
                variant: AwsSdkErrorVariant::DispatchFailure,
                dispatch_kind: Some(dispatch_kind),
                http_status: None,
                aws_error_code: None,
                aws_error_message: None,
                aws_request_id: None,
            }
        }
        SdkError::ResponseError(err) => AwsErrorContext {
            variant: AwsSdkErrorVariant::ResponseError,
            dispatch_kind: None,
            http_status: Some(err.raw().status().as_u16()),
            aws_error_code: None,
            aws_error_message: None,
            aws_request_id: error.request_id(),
        },
        SdkError::ServiceError(err) => {
            let service_err = err.err();
            AwsErrorContext {
                variant: AwsSdkErrorVariant::ServiceError,
                dispatch_kind: None,
                http_status: Some(err.raw().status().as_u16()),
                aws_error_code: service_err.code(),
                aws_error_message: service_err.message(),
                aws_request_id: error.request_id(),
            }
        }
        _ => AwsErrorContext {
            variant: AwsSdkErrorVariant::Unknown,
            dispatch_kind: None,
            http_status: None,
            aws_error_code: None,
            aws_error_message: None,
            aws_request_id: None,
        },
    }
}

/// Classify an error for logging and metrics.
/// Uses error code first, HTTP status as fallback, SdkError variant as last resort.
pub fn classify_error(ctx: &AwsErrorContext<'_>) -> AwsErrorClass {
    // 1. Prefer error code (most specific)
    if let Some(code) = ctx.aws_error_code {
        if AUTH_CODES.contains(&code) {
            return AwsErrorClass::Auth;
        }
        if NOT_FOUND_CODES.contains(&code) {
            return AwsErrorClass::NotFound;
        }
        if THROTTLING_CODES.contains(&code) {
            return AwsErrorClass::Throttling;
        }
    }
    // 2. Fall back to HTTP status
    if let Some(status) = ctx.http_status {
        return match status {
            401 | 403 => AwsErrorClass::Auth,
            404 => AwsErrorClass::NotFound,
            429 => AwsErrorClass::Throttling,
            400..=499 => AwsErrorClass::RequestError,
            500..=599 => AwsErrorClass::ServiceError,
            _ => AwsErrorClass::Unknown,
        };
    }
    // 3. Fall back to SdkError variant
    match ctx.variant {
        AwsSdkErrorVariant::TimeoutError | AwsSdkErrorVariant::DispatchFailure => {
            AwsErrorClass::Connectivity
        }
        AwsSdkErrorVariant::ConstructionFailure => AwsErrorClass::Configuration,
        _ => AwsErrorClass::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_smithy_runtime_api::client::result::ConnectorError;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::error::ErrorMetadata;

    /// Helper: build an HttpResponse with the given status code.
    fn http_response(status: u16) -> HttpResponse {
        let http_resp = http::Response::builder()
            .status(status)
            .body(SdkBody::empty())
            .unwrap();
        HttpResponse::try_from(http_resp).unwrap()
    }

    /// Helper: build an HttpResponse with a request ID header.
    fn http_response_with_request_id(status: u16, request_id: &str) -> HttpResponse {
        let http_resp = http::Response::builder()
            .status(status)
            .header("x-amz-request-id", request_id)
            .body(SdkBody::empty())
            .unwrap();
        HttpResponse::try_from(http_resp).unwrap()
    }

    // -- ServiceError tests --

    #[test]
    fn service_error_access_denied_classifies_as_auth() {
        let meta = ErrorMetadata::builder()
            .code("AccessDenied")
            .message("Access Denied")
            .build();
        let raw = http_response_with_request_id(403, "abc-123");
        let err: SdkError<ErrorMetadata, HttpResponse> = SdkError::service_error(meta, raw);

        let ctx = extract_error_context(&err);

        assert_eq!(ctx.variant, AwsSdkErrorVariant::ServiceError);
        assert_eq!(ctx.http_status, Some(403));
        assert_eq!(ctx.aws_error_code, Some("AccessDenied"));
        assert_eq!(ctx.aws_error_message, Some("Access Denied"));
        assert_eq!(ctx.aws_request_id, Some("abc-123"));
        assert!(ctx.dispatch_kind.is_none());
        assert_eq!(classify_error(&ctx), AwsErrorClass::Auth);
    }

    #[test]
    fn service_error_no_such_key_classifies_as_not_found() {
        let meta = ErrorMetadata::builder()
            .code("NoSuchKey")
            .message("The specified key does not exist.")
            .build();
        let raw = http_response(404);
        let err: SdkError<ErrorMetadata, HttpResponse> = SdkError::service_error(meta, raw);

        let ctx = extract_error_context(&err);
        assert_eq!(ctx.aws_error_code, Some("NoSuchKey"));
        assert_eq!(ctx.http_status, Some(404));
        assert_eq!(classify_error(&ctx), AwsErrorClass::NotFound);
    }

    #[test]
    fn service_error_throttling_classifies_as_throttling() {
        let meta = ErrorMetadata::builder()
            .code("ThrottlingException")
            .build();
        let raw = http_response(429);
        let err: SdkError<ErrorMetadata, HttpResponse> = SdkError::service_error(meta, raw);

        let ctx = extract_error_context(&err);
        assert_eq!(classify_error(&ctx), AwsErrorClass::Throttling);
    }

    #[test]
    fn service_error_no_code_http_429_classifies_as_throttling() {
        let meta = ErrorMetadata::builder().build();
        let raw = http_response(429);
        let err: SdkError<ErrorMetadata, HttpResponse> = SdkError::service_error(meta, raw);

        let ctx = extract_error_context(&err);
        assert!(ctx.aws_error_code.is_none());
        assert_eq!(ctx.http_status, Some(429));
        assert_eq!(classify_error(&ctx), AwsErrorClass::Throttling);
    }

    #[test]
    fn service_error_no_code_http_403_classifies_as_auth() {
        let meta = ErrorMetadata::builder().build();
        let raw = http_response(403);
        let err: SdkError<ErrorMetadata, HttpResponse> = SdkError::service_error(meta, raw);

        let ctx = extract_error_context(&err);
        assert_eq!(classify_error(&ctx), AwsErrorClass::Auth);
    }

    #[test]
    fn service_error_http_500_classifies_as_service_error() {
        let meta = ErrorMetadata::builder().build();
        let raw = http_response(500);
        let err: SdkError<ErrorMetadata, HttpResponse> = SdkError::service_error(meta, raw);

        let ctx = extract_error_context(&err);
        assert_eq!(classify_error(&ctx), AwsErrorClass::ServiceError);
    }

    #[test]
    fn service_error_http_400_classifies_as_request_error() {
        let meta = ErrorMetadata::builder().build();
        let raw = http_response(400);
        let err: SdkError<ErrorMetadata, HttpResponse> = SdkError::service_error(meta, raw);

        let ctx = extract_error_context(&err);
        assert_eq!(classify_error(&ctx), AwsErrorClass::RequestError);
    }

    // -- DispatchFailure tests --

    #[test]
    fn dispatch_failure_timeout() {
        let connector_err =
            ConnectorError::timeout("connection timed out".into());
        let err: SdkError<ErrorMetadata, HttpResponse> =
            SdkError::dispatch_failure(connector_err);

        let ctx = extract_error_context(&err);
        assert_eq!(ctx.variant, AwsSdkErrorVariant::DispatchFailure);
        assert_eq!(ctx.dispatch_kind, Some(AwsDispatchKind::Timeout));
        assert!(ctx.http_status.is_none());
        assert!(ctx.aws_error_code.is_none());
        assert_eq!(classify_error(&ctx), AwsErrorClass::Connectivity);
    }

    #[test]
    fn dispatch_failure_io() {
        let connector_err = ConnectorError::io("dns resolution failed".into());
        let err: SdkError<ErrorMetadata, HttpResponse> =
            SdkError::dispatch_failure(connector_err);

        let ctx = extract_error_context(&err);
        assert_eq!(ctx.dispatch_kind, Some(AwsDispatchKind::Io));
        assert_eq!(classify_error(&ctx), AwsErrorClass::Connectivity);
    }

    #[test]
    fn dispatch_failure_user() {
        let connector_err =
            ConnectorError::user("invalid request".into());
        let err: SdkError<ErrorMetadata, HttpResponse> =
            SdkError::dispatch_failure(connector_err);

        let ctx = extract_error_context(&err);
        assert_eq!(ctx.dispatch_kind, Some(AwsDispatchKind::User));
    }

    #[test]
    fn dispatch_failure_other() {
        let connector_err =
            ConnectorError::other("unknown error".into(), None);
        let err: SdkError<ErrorMetadata, HttpResponse> =
            SdkError::dispatch_failure(connector_err);

        let ctx = extract_error_context(&err);
        assert_eq!(ctx.dispatch_kind, Some(AwsDispatchKind::Other));
    }

    // -- ConstructionFailure / TimeoutError tests --

    #[test]
    fn construction_failure_classifies_as_configuration() {
        let err: SdkError<ErrorMetadata, HttpResponse> =
            SdkError::construction_failure("bad config");

        let ctx = extract_error_context(&err);
        assert_eq!(ctx.variant, AwsSdkErrorVariant::ConstructionFailure);
        assert!(ctx.http_status.is_none());
        assert_eq!(classify_error(&ctx), AwsErrorClass::Configuration);
    }

    #[test]
    fn timeout_error_classifies_as_connectivity() {
        let err: SdkError<ErrorMetadata, HttpResponse> =
            SdkError::timeout_error("request timed out");

        let ctx = extract_error_context(&err);
        assert_eq!(ctx.variant, AwsSdkErrorVariant::TimeoutError);
        assert_eq!(classify_error(&ctx), AwsErrorClass::Connectivity);
    }

    // -- ResponseError tests --

    #[test]
    fn response_error_extracts_http_status() {
        let raw = http_response_with_request_id(502, "req-456");
        let err: SdkError<ErrorMetadata, HttpResponse> =
            SdkError::response_error("parse failure", raw);

        let ctx = extract_error_context(&err);
        assert_eq!(ctx.variant, AwsSdkErrorVariant::ResponseError);
        assert_eq!(ctx.http_status, Some(502));
        assert!(ctx.aws_error_code.is_none());
        assert_eq!(classify_error(&ctx), AwsErrorClass::ServiceError);
    }

    // -- Error code list coverage tests --

    #[test]
    fn all_auth_codes_classify_as_auth() {
        for code in AUTH_CODES {
            let meta = ErrorMetadata::builder().code(*code).build();
            let raw = http_response(200);
            let err: SdkError<ErrorMetadata, HttpResponse> =
                SdkError::service_error(meta, raw);
            let ctx = extract_error_context(&err);
            assert_eq!(
                classify_error(&ctx),
                AwsErrorClass::Auth,
                "Expected Auth for code {code}"
            );
        }
    }

    #[test]
    fn all_not_found_codes_classify_as_not_found() {
        for code in NOT_FOUND_CODES {
            let meta = ErrorMetadata::builder().code(*code).build();
            let raw = http_response(200);
            let err: SdkError<ErrorMetadata, HttpResponse> =
                SdkError::service_error(meta, raw);
            let ctx = extract_error_context(&err);
            assert_eq!(
                classify_error(&ctx),
                AwsErrorClass::NotFound,
                "Expected NotFound for code {code}"
            );
        }
    }

    #[test]
    fn all_throttling_codes_classify_as_throttling() {
        for code in THROTTLING_CODES {
            let meta = ErrorMetadata::builder().code(*code).build();
            let raw = http_response(200);
            let err: SdkError<ErrorMetadata, HttpResponse> =
                SdkError::service_error(meta, raw);
            let ctx = extract_error_context(&err);
            assert_eq!(
                classify_error(&ctx),
                AwsErrorClass::Throttling,
                "Expected Throttling for code {code}"
            );
        }
    }

    #[test]
    fn sqs_queue_not_found_codes_classify_as_not_found() {
        for code in &["QueueDoesNotExist", "AWS.SimpleQueueService.NonExistentQueue"] {
            let meta = ErrorMetadata::builder().code(*code).build();
            let raw = http_response(400);
            let err: SdkError<ErrorMetadata, HttpResponse> =
                SdkError::service_error(meta, raw);
            let ctx = extract_error_context(&err);
            assert_eq!(
                classify_error(&ctx),
                AwsErrorClass::NotFound,
                "Expected NotFound for code {code}"
            );
        }
    }

    #[test]
    fn no_code_http_404_classifies_as_not_found() {
        let meta = ErrorMetadata::builder().build();
        let raw = http_response(404);
        let err: SdkError<ErrorMetadata, HttpResponse> = SdkError::service_error(meta, raw);

        let ctx = extract_error_context(&err);
        assert!(ctx.aws_error_code.is_none());
        assert_eq!(classify_error(&ctx), AwsErrorClass::NotFound);
    }

    #[test]
    fn error_code_takes_precedence_over_http_status() {
        let meta = ErrorMetadata::builder().code("AccessDenied").build();
        let raw = http_response(500);
        let err: SdkError<ErrorMetadata, HttpResponse> = SdkError::service_error(meta, raw);

        let ctx = extract_error_context(&err);
        assert_eq!(
            classify_error(&ctx),
            AwsErrorClass::Auth,
            "Error code should take precedence over HTTP 500 status"
        );
    }
}
