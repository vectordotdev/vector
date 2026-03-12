use snafu::Snafu;

/// Error types for the gRPC client
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum Error {
    #[snafu(display("Connection error: {}", source))]
    Connection { source: tonic::transport::Error },

    #[snafu(display("gRPC error: {}", source))]
    Grpc { source: tonic::Status },

    #[snafu(display("Invalid URL: {}", message))]
    InvalidUrl { message: String },

    #[snafu(display("Not connected to gRPC server"))]
    NotConnected,

    #[snafu(display("Stream error: {}", message))]
    Stream { message: String },
}

impl From<tonic::Status> for Error {
    fn from(source: tonic::Status) -> Self {
        Error::Grpc { source }
    }
}

impl From<tonic::transport::Error> for Error {
    fn from(source: tonic::transport::Error) -> Self {
        Error::Connection { source }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::NotConnected;
        assert_eq!(err.to_string(), "Not connected to gRPC server");

        let err = Error::InvalidUrl {
            message: "invalid format".to_string(),
        };
        assert_eq!(err.to_string(), "Invalid URL: invalid format");
    }

    #[test]
    fn test_error_from_tonic_status() {
        let status = tonic::Status::unavailable("service down");
        let err: Error = status.into();
        assert!(matches!(err, Error::Grpc { .. }));
    }
}
