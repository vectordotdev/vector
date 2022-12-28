use std::time::SystemTimeError;

use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum DatabendError {
    #[snafu(display("Server responded with an error: {}, {}", code, message))]
    Server { code: i64, message: String },

    #[snafu(display("Failed to make HTTP(S) request: {}: {}", message, error))]
    Request {
        error: crate::http::HttpError,
        message: String,
    },

    #[snafu(display("Failed to build HTTP request: {}: {}", message, error))]
    Http { error: http::Error, message: String },

    #[snafu(display("Hyper internal error: {}: {}", message, error))]
    Hyper {
        error: hyper::Error,
        message: String,
    },
    #[snafu(display("Failed to encode request body: {}: {}", message, error))]
    Encode {
        error: serde_json::Error,
        message: String,
    },
    #[snafu(display("Failed to decode response body: {}: {}", message, error))]
    Decode {
        error: serde_json::Error,
        message: String,
    },
    #[snafu(display("SystemTime error: {}: {}", message, error))]
    SystemTime {
        error: SystemTimeError,
        message: String,
    },
}
