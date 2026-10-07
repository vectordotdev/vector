use bytes::BytesMut;
use http::{HeaderValue, header::CONTENT_TYPE};
use prost::Message;
use warp::{Reply, reply::Response};

use super::status::Status;

/// If a type fails to be encoded as Protobuf, the error is logged at the
/// `error` level, and the returned `impl Reply` will be an empty
/// `500 Internal Server Error` response.
// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::needless_pass_by_value,
    reason = "Keep ownership and drop timing unchanged during the lint rollout."
)]
pub fn protobuf<T>(val: T) -> Protobuf
where
    T: Message,
{
    let mut buf = BytesMut::with_capacity(1024);
    Protobuf {
        inner: val.encode(&mut buf).map(|()| buf.to_vec()).map_err(|err| {
            error!("Failed to encode value: {err}");
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
        if let Ok(body) = self.inner {
            let mut res = Response::new(body.into());
            res.headers_mut().insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/x-protobuf"),
            );
            res
        } else {
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
