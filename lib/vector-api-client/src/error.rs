use snafu::Snafu;

/// Error types for the gRPC client
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum Error {
    /// The HTTP/2 transport could not be set up, for example because TLS failed to initialize.
    #[snafu(display("Connection setup error: {}", source))]
    Connection {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

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

pub type Result<T> = std::result::Result<T, Error>;
