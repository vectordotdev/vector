#![deny(missing_docs)]

use tokio_util::codec::LinesCodecError;

/// An error that occurs while decoding a stream.
pub trait StreamDecodingError {
    /// Whether it is reasonable to assume that continuing to read from the
    /// stream in which this error occurred will not result in an indefinite
    /// hang up.
    ///
    /// This can occur e.g. when reading the header of a length-delimited codec
    /// failed and it can no longer be determined where the next header starts.
    fn can_continue(&self) -> bool;
}

impl StreamDecodingError for LinesCodecError {
    fn can_continue(&self) -> bool {
        match self {
            LinesCodecError::MaxLineLengthExceeded => true,
            LinesCodecError::Io(error) => error.can_continue(),
        }
    }
}

impl StreamDecodingError for std::io::Error {
    fn can_continue(&self) -> bool {
        false
    }
}
