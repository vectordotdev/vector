use http::StatusCode;
use serde::Serialize;
use std::error::Error;
use std::fmt;

#[derive(Serialize, Debug)]
pub struct ErrorMessage {
    code: u16,
    message: String,
}

impl ErrorMessage {
    pub fn new(code: StatusCode, message: String) -> Self {
        ErrorMessage {
            code: code.as_u16(),
            message,
        }
    }

    pub fn status_code(&self) -> StatusCode {
        StatusCode::from_u16(self.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
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
