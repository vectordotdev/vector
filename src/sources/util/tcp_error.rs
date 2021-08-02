use tokio_util::codec::LinesCodecError;

pub trait TcpError {
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
