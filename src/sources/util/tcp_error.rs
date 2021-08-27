#![deny(missing_docs)]

use tokio_util::codec::LinesCodecError;

/// An error that occurs in the context of TCP connections.
pub trait TcpError {
    /// The TCP error has been fatal, the TCP stream should no longer be read
    /// since it will hang up indefinitely.
    ///
    /// This can occur e.g. when reading the header of a length-delimited codec
    /// failed and it can no longer be determined where the next header starts.
    fn is_fatal(&self) -> bool;
}

impl TcpError for LinesCodecError {
    fn is_fatal(&self) -> bool {
        false
    }
}

impl TcpError for std::io::Error {
    fn is_fatal(&self) -> bool {
        true
    }
}
