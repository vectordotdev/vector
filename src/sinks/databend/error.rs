use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum DatabendError {
    #[snafu(display("Server responded with an error: {}, {}", code, message))]
    Server { code: u16, message: String },

    #[snafu(display("Parse response failed: {}", response))]
    Parser { response: String },

    #[snafu(display("Client error: {}", message))]
    Client { message: String },

    #[snafu(display("Invalid config: {}", message))]
    InvalidConfig { message: String },
}

impl From<crate::Error> for DatabendError {
    fn from(error: crate::Error) -> Self {
        Self::Client {
            message: error.to_string(),
        }
    }
}

impl From<serde_json::Error> for DatabendError {
    fn from(error: serde_json::Error) -> Self {
        Self::Client {
            message: error.to_string(),
        }
    }
}

impl From<http::Error> for DatabendError {
    fn from(error: http::Error) -> Self {
        Self::Client {
            message: error.to_string(),
        }
    }
}

impl From<hyper::Error> for DatabendError {
    fn from(error: hyper::Error) -> Self {
        Self::Client {
            message: error.to_string(),
        }
    }
}

impl From<crate::http::HttpError> for DatabendError {
    fn from(error: crate::http::HttpError) -> Self {
        Self::Client {
            message: error.to_string(),
        }
    }
}
