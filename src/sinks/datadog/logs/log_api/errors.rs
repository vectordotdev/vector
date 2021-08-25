use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum LogApiError {}

#[derive(Debug, Snafu)]
pub enum FlushError {
    #[snafu(display("Unable to flush to API owing to the following error: {}", error))]
    Http { error: http::Error },
    #[snafu(display("Unable to flush to API owing to the following error: {}", error))]
    Io { error: std::io::Error },
}

impl From<http::Error> for FlushError {
    fn from(error: http::Error) -> FlushError {
        FlushError::Http { error }
    }
}

impl From<std::io::Error> for FlushError {
    fn from(error: std::io::Error) -> FlushError {
        FlushError::Io { error }
    }
}
