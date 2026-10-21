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

    #[snafu(display("Server is not serving (status: {})", status))]
    NotServing { status: i32 },

    #[snafu(display("Stream error: {}", message))]
    Stream { message: String },
}

impl Error {
    /// Returns true if this error is permanent and should not be retried.
    #[must_use]
    pub fn is_fatal(&self) -> bool {
        match self {
            Error::InvalidUrl { .. } => true,
            Error::Grpc { source } => matches!(
                source.code(),
                tonic::Code::InvalidArgument
                    | tonic::Code::OutOfRange
                    | tonic::Code::NotFound
                    | tonic::Code::PermissionDenied
                    | tonic::Code::Unauthenticated
                    | tonic::Code::Unimplemented
            ),
            _ => false,
        }
    }
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
