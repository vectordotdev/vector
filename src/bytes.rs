use serde::ser::{SerializeMap, Serializer};
use std::{borrow::Cow, collections::HashMap, string::FromUtf8Error};
use string_cache::DefaultAtom as Atom;

pub use bytes::{Buf, BufMut, Bytes, BytesMut, IntoBuf};

/// An extension trait for `bytes::Buf`
///
/// Currently this extension trait provides methods to produce `String`s from
/// a `Buf`.`
pub trait BytesExt {
    fn as_utf8_lossy<'a>(&'a self) -> Cow<'a, str>;
    fn into_string(self) -> Result<String, FromUtf8Error>;
}

impl BytesExt for Bytes {
    fn as_utf8_lossy<'a>(&'a self) -> Cow<'a, str> {
        String::from_utf8_lossy(&self[..])
    }

    fn into_string(self) -> Result<String, FromUtf8Error> {
        let buf = self.into_buf().collect::<Vec<u8>>();
        String::from_utf8(buf)
    }
}

pub fn serialize<S>(b: &Bytes, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    ser.serialize_str(&String::from_utf8_lossy(&b[..]))
}

pub fn serialize_map<S>(m: &HashMap<Atom, Bytes>, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map = ser.serialize_map(Some(m.len()))?;
    for (k, v) in m {
        map.serialize_entry(k, &String::from_utf8_lossy(&v[..]))?;
    }
    map.end()
}

#[cfg(test)]
mod tests {
    use super::{Bytes, BytesExt};

    #[test]
    fn buf_into_str_lossy() {
        let buf = Bytes::from("hello world");
        assert_eq!(buf.as_utf8_lossy(), "hello world".to_string())
    }

    #[test]
    fn buf_into_string() {
        let buf = Bytes::from("hello world");
        let string = buf.into_string().unwrap();
        assert_eq!(string, "hello world".to_string())
    }
}
