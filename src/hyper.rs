use bytes::{Bytes, BytesMut};
use futures::TryStreamExt;
use hyper::{Body, Error};

// TODO: Can be eliminated once we start using `bytes` crate with the same version as `hyper`.
pub async fn body_to_bytes(body: Body) -> Result<Bytes, Error> {
    body
        // hyper::body::to_body
        .try_fold(BytesMut::new(), |mut store, bytes| async move {
            store.extend_from_slice(&bytes);
            Ok(store)
        })
        .await
        .map(Into::into)
}
