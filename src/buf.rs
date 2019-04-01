use bytes::Buf;
use std::string::FromUtf8Error;

/// An extension trait for `bytes::Buf`
///
/// Currently this extension trait provides methods to produce `String`s from
/// a `Buf`.`
pub trait BufExt: Buf {
    fn into_string_lossy(self) -> String
    where
        Self: Sized,
    {
        let buf = self.collect::<Vec<u8>>();
        String::from_utf8_lossy(&buf[..]).into_owned()
    }

    fn into_string(self) -> Result<String, FromUtf8Error>
    where
        Self: Sized,
    {
        let buf = self.collect::<Vec<u8>>();
        String::from_utf8(buf)
    }
}

impl<T: Buf> BufExt for T {}

#[cfg(test)]
mod tests {
    use super::BufExt;
    use bytes::{Bytes, IntoBuf};

    #[test]
    fn buf_into_str_lossy() {
        let buf = Bytes::from("hello world").into_buf();
        assert_eq!(buf.into_string_lossy(), "hello world".to_string())
    }

    #[test]
    fn buf_into_string() {
        let buf = Bytes::from("hello world").into_buf();
        let string = buf.into_string().unwrap();
        assert_eq!(string, "hello world".to_string())
    }
}
