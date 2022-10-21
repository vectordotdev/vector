use std::fmt;

use bytes::BytesMut;
use http::{header::CONTENT_TYPE, HeaderValue};
use prost::Message;
use warp::{reply::Response, Reply};

use super::status::Status;

/// If a type fails to be encoded as Protobuf, the error is logged at the
/// `error` level, and the returned `impl Reply` will be an empty
/// `500 Internal Server Error` response.
pub fn protobuf<T>(val: T) -> Protobuf
where
    T: Message,
{
    let mut buf = BytesMut::with_capacity(1024);
    Protobuf {
        inner: val.encode(&mut buf).map(|_| buf.to_vec()).map_err(|err| {
            error!("Failed to encode value: {}", err);
        }),
    }
}

/// A Protobuf formatted reply.
#[allow(missing_debug_implementations)]
pub struct Protobuf {
    inner: Result<Vec<u8>, ()>,
}

impl Reply for Protobuf {
    #[inline]
    fn into_response(self) -> Response {
        match self.inner {
            Ok(body) => {
                let mut res = Response::new(body.into());
                res.headers_mut().insert(
                    CONTENT_TYPE,
                    HeaderValue::from_static("application/x-protobuf"),
                );
                res
            }
            Err(()) => {
                let status = Status {
                    message: "Failed to encode error message".into(),
                    ..Default::default()
                };
                let mut res = Response::new(status.encode_to_vec().into());
                res.headers_mut().insert(
                    CONTENT_TYPE,
                    HeaderValue::from_static("application/x-protobuf"),
                );
                res
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct ReplyProtobufError;

impl fmt::Display for ReplyProtobufError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("http::reply::protobuf() failed")
    }
}

impl std::error::Error for ReplyProtobufError {}
