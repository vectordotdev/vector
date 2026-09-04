use snafu::Snafu;
use warp::http::StatusCode;

use super::handlers::RecordDecodeError;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum RequestError {
    #[snafu(display(
        "Missing access key. X-Amz-Firehose-Access-Key required for request: {}",
        request_id
    ))]
    AccessKeyMissing { request_id: String },
    #[snafu(display(
        "Invalid access key. X-Amz-Firehose-Access-Key does not match configured access_key for request: {}",
        request_id
    ))]
    AccessKeyInvalid { request_id: String },
    #[snafu(display("Could not parse incoming request {}: {}", request_id, source))]
    Parse {
        source: serde_json::error::Error,
        request_id: String,
    },
    #[snafu(display(
        "Could not parse records from incoming request {}: {}",
        request_id,
        source
    ))]
    ParseRecords {
        source: RecordDecodeError,
        request_id: String,
    },
    #[snafu(display("Could not decode record for request {}: {}", request_id, source))]
    Decode {
        source: std::io::Error,
        request_id: String,
    },
    #[snafu(display("Could not forward events for request {request_id}, downstream is closed"))]
    ShuttingDown { request_id: String },
    #[snafu(display("Unsupported encoding: {}", encoding))]
    UnsupportedEncoding {
        encoding: String,
        request_id: String,
    },
    #[snafu(display("Unsupported protocol version: {}", version))]
    UnsupportedProtocolVersion { version: String },
    #[snafu(display("Delivery errored"))]
    DeliveryErrored { request_id: String },
    #[snafu(display("Delivery failed"))]
    DeliveryFailed { request_id: String },
}

impl warp::reject::Reject for RequestError {}

impl RequestError {
    pub const fn status(&self) -> StatusCode {
        use RequestError::*;
        match *self {
            AccessKeyMissing { .. } => StatusCode::UNAUTHORIZED,
            AccessKeyInvalid { .. } => StatusCode::UNAUTHORIZED,
            Parse { .. } => StatusCode::UNAUTHORIZED,
            UnsupportedEncoding { .. } => StatusCode::BAD_REQUEST,
            ParseRecords { .. } => StatusCode::BAD_REQUEST,
            Decode { .. } => StatusCode::BAD_REQUEST,
            ShuttingDown { .. } => StatusCode::SERVICE_UNAVAILABLE,
            UnsupportedProtocolVersion { .. } => StatusCode::BAD_REQUEST,
            DeliveryErrored { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            DeliveryFailed { .. } => StatusCode::NOT_ACCEPTABLE,
        }
    }

    pub const fn request_id(&self) -> Option<&str> {
        use RequestError::*;
        match self {
            AccessKeyMissing { request_id, .. }
            | AccessKeyInvalid { request_id, .. }
            | Parse { request_id, .. }
            | UnsupportedEncoding { request_id, .. }
            | ParseRecords { request_id, .. }
            | Decode { request_id, .. }
            | ShuttingDown { request_id, .. }
            | DeliveryErrored { request_id }
            | DeliveryFailed { request_id } => Some(request_id.as_str()),
            UnsupportedProtocolVersion { .. } => None,
        }
    }
}
