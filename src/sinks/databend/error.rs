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

    #[snafu(display("Component Internal error: {}", message))]
    Internal { message: String },
}

impl From<crate::Error> for DatabendError {
    fn from(error: crate::Error) -> Self {
        Self::Internal {
            message: error.to_string(),
        }
    }
}

impl From<serde_json::Error> for DatabendError {
    fn from(error: serde_json::Error) -> Self {
        Self::Internal {
            message: error.to_string(),
        }
    }
}

impl From<std::time::SystemTimeError> for DatabendError {
    fn from(error: std::time::SystemTimeError) -> Self {
        Self::Internal {
            message: error.to_string(),
        }
    }
}

impl From<http::Error> for DatabendError {
    fn from(error: http::Error) -> Self {
        Self::Internal {
            message: error.to_string(),
        }
    }
}

impl From<hyper::Error> for DatabendError {
    fn from(error: hyper::Error) -> Self {
        Self::Internal {
            message: error.to_string(),
        }
    }
}
