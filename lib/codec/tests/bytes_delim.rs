extern crate bytes;
extern crate codec;
extern crate tokio_codec;

use bytes::{BufMut, BytesMut};
use codec::BytesDelimitedCodec;
use tokio_codec::{Decoder, Encoder};

#[test]
fn bytes_delim_decod() {
    let mut codec = BytesDelimitedCodec::new(b'\n');
    let buf = &mut BytesMut::new();
    buf.put_slice(b"abc\n");
    assert_eq!(Some("abc".into()), codec.decode(buf).unwrap());
}

#[test]
fn bytes_delim_encode() {
    let mut codec = BytesDelimitedCodec::new(b'\n');

    let mut buf = BytesMut::new();
    codec.encode("abc".into(), &mut buf).unwrap();

    assert_eq!(b"abc\n", &buf[..]);
}

#[test]
fn bytes_decode_max_length() {
    const MAX_LENGTH: usize = 6;

    let mut codec = BytesDelimitedCodec::new_with_max_length(b'\n', MAX_LENGTH);
    let buf = &mut BytesMut::new();

    buf.reserve(200);
    // limit is 6 so this should fail
    buf.put_slice(b"1234567\n123456\n123412314");

    assert!(codec.decode(buf).is_err());
    assert!(codec.decode(buf).is_ok());
    assert!(codec.decode_eof(buf).is_err());
}
