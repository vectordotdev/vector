use std::{error::Error, fmt};

use serde::Serialize;

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
    #[allow(unused)] // triggered by check-component-features
    pub fn new(code: http::StatusCode, message: String) -> Self {
        ErrorMessage {
            code: code.as_u16(),
            message,
        }
    }

    #[allow(unused)] // triggered by check-component-features
    pub fn status_code(&self) -> http::StatusCode {
        http::StatusCode::from_u16(self.code).unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR)
    }
}

#[cfg(feature = "sources-utils-http-prelude")]
impl ErrorMessage {
    pub const fn code(&self) -> u16 {
        self.code
    }

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
