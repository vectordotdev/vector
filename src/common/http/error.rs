use std::{error::Error, fmt};

use serde::Serialize;

/// HTTP error, containing HTTP status code and a message
#[derive(Serialize, Debug)]
pub struct ErrorMessage {
    code: u16,
    message: String,
}

#[cfg(any(
    feature = "sources-utils-http-prelude",
    feature = "sources-utils-http-auth",
    feature = "sources-utils-http-encoding",
    feature = "sources-datadog_agent"
))]
impl ErrorMessage {
    /// Create a new `ErrorMessage` from HTTP status code and a message
    #[allow(unused)] // triggered by check-component-features
    pub fn new(code: http::StatusCode, message: String) -> Self {
        ErrorMessage {
            code: code.as_u16(),
            message,
        }
    }

    /// Returns the HTTP status code
    #[allow(unused)] // triggered by check-component-features
    pub fn status_code(&self) -> http::StatusCode {
        http::StatusCode::from_u16(self.code).unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR)
    }
}

#[cfg(feature = "sources-utils-http-prelude")]
impl ErrorMessage {
    /// Returns the raw HTTP status code
    pub const fn code(&self) -> u16 {
        self.code
    }

    /// Returns the error message
    pub fn message(&self) -> &str {
        self.message.as_str()
    }
}

impl Error for ErrorMessage {}

impl fmt::Display for ErrorMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl warp::reject::Reject for ErrorMessage {}
