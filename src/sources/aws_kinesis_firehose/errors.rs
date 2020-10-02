use crate::event::Event;
use snafu::Snafu;
use warp::http::StatusCode;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub")]
pub enum RequestError {
    #[snafu(display("X-Amz-Firehose-Access-Key required for request: {}", request_id))]
    AccessKeyMissing { request_id: String },
    #[snafu(display(
        "X-Amz-Firehose-Access-Key does not match configured key for request: {}",
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
        source: std::io::Error,
        request_id: String,
    },
    #[snafu(display("Could not decode record for request {}: {}", request_id, source))]
    Decode {
        source: std::io::Error,
        request_id: String,
    },
    #[snafu(display(
        "Could not forward events for request {}, downstream is closed: {}",
        request_id,
        source
    ))]
    ShuttingDown {
        source: futures01::sync::mpsc::SendError<Event>,
        request_id: String,
    },
    #[snafu(display("Unsupported encoding: {}", encoding))]
    UnsupportedEncoding {
        encoding: String,
        request_id: String,
    },
    #[snafu(display("Unsupported protocol version: {}", version))]
    UnsupportedProtocolVersion { version: String },
}

impl warp::reject::Reject for RequestError {}

impl RequestError {
    pub fn status(&self) -> StatusCode {
        match *self {
            RequestError::AccessKeyMissing { .. } => StatusCode::UNAUTHORIZED,
            RequestError::AccessKeyInvalid { .. } => StatusCode::UNAUTHORIZED,
            RequestError::Parse { .. } => StatusCode::UNAUTHORIZED,
            RequestError::UnsupportedEncoding { .. } => StatusCode::BAD_REQUEST,
            RequestError::ParseRecords { .. } => StatusCode::BAD_REQUEST,
            RequestError::Decode { .. } => StatusCode::BAD_REQUEST,
            RequestError::ShuttingDown { .. } => StatusCode::SERVICE_UNAVAILABLE,
            RequestError::UnsupportedProtocolVersion { .. } => StatusCode::BAD_REQUEST,
        }
    }

    pub fn request_id(&self) -> Option<String> {
        match *self {
            RequestError::AccessKeyMissing { ref request_id, .. } => Some(request_id),
            RequestError::AccessKeyInvalid { ref request_id, .. } => Some(request_id),
            RequestError::Parse { ref request_id, .. } => Some(request_id),
            RequestError::UnsupportedEncoding { ref request_id, .. } => Some(request_id),
            RequestError::ParseRecords { ref request_id, .. } => Some(request_id),
            RequestError::Decode { ref request_id, .. } => Some(request_id),
            RequestError::ShuttingDown { ref request_id, .. } => Some(request_id),
            RequestError::UnsupportedProtocolVersion { .. } => None,
        }
        .map(|s| s.clone())
    }
}

impl From<RequestError> for warp::reject::Rejection {
    fn from(error: RequestError) -> Self {
        warp::reject::custom(error)
    }
}
