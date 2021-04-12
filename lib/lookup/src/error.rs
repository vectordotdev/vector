use snafu::Snafu;
pub use LookupError::*;

#[derive(Debug, Snafu)]
pub enum LookupError {
    #[snafu(display("Expected array index, did not get one."))]
    MissingIndex,
    #[snafu(display("Missing inner of quoted segment."))]
    MissingInnerSegment,
    #[snafu(display("No tokens found to parse."))]
    NoTokens,
}
