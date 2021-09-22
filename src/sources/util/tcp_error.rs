#![deny(missing_docs)]

use tokio_util::codec::LinesCodecError;

// TODO: Rename to reflect that this error can generally appear in stream based
// decoding operations rather than only in TCP streams.
//
/// An error that occurs in the context of TCP connections.
pub trait TcpError {
    /// Whether it is reasonable to assume that continuing to read from the TCP
    /// stream in which this error occurred will not result in an indefinite
    /// hang up.
    ///
    /// This can occur e.g. when reading the header of a length-delimited codec
    /// failed and it can no longer be determined where the next header starts.
    fn can_continue(&self) -> bool;
}

impl TcpError for LinesCodecError {
    fn can_continue(&self) -> bool {
        match self {
            LinesCodecError::MaxLineLengthExceeded => true,
            LinesCodecError::Io(error) => error.can_continue(),
        }
    }
}

impl TcpError for std::io::Error {
    fn can_continue(&self) -> bool {
        false
    }
}
