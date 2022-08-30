use hyper::StatusCode;
use thiserror::Error;
use tuf::metadata::TargetPath;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("openssl initialization error: {}", .0)]
    OpenSsl(String),
    #[error("tuf library error")]
    Tuf(#[from] tuf::Error),
    #[error("http stream error")]
    HttpStream(#[from] hyper::Error),
    #[error("http request building error")]
    HttpReqBuild(#[from] hyper::http::Error),
    #[error("unexpected http status {}, body {:?}", .status, .body)]
    HttpUnexpectedStatus { status: StatusCode, body: String },
    #[error("missing tuf snapshot data")]
    MissingSnapshotData,
    #[error("missing tuf target data")]
    MissingTargetData,
    #[error("missing config metas")]
    MissingConfigMetas,
    #[error("missing director metas")]
    MissingDirectorMetas,
    #[error("unknown target path {}", .0)]
    UnknownTarget(TargetPath),
    #[error("IO error")]
    Io(#[from] std::io::Error),
    #[error("protobuf decoding error")]
    ProtobufDecode(#[from] prost::DecodeError),
    #[error("mismatched target length")]
    BadLength,
    #[error("no supported hash algorithms present")]
    NoSupportedHashes,
    #[error("mismatched target hash")]
    BadHash,
    #[error("missing custom version data for target")]
    MissingTargetVersion,
}
