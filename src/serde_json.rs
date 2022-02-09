use bytes::{BufMut, BytesMut};
use serde::Serialize;

/// Serialize the given data structure as JSON to `BytesMut`.
///
/// # Errors
///
/// Serialization can fail if `T`'s implementation of `Serialize` decides to
/// fail, or if `T` contains a map with non-string keys.
pub fn to_bytes<T>(value: &T) -> serde_json::Result<BytesMut>
where
    T: ?Sized + Serialize,
{
    let mut bytes = BytesMut::new();
    serde_json::to_writer((&mut bytes).writer(), value)?;
    Ok(bytes)
}
