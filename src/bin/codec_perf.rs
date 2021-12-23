use bytes::BytesMut;
use tokio_util::codec::Decoder;
use vector::codecs::{self, BytesDeserializer, CharacterDelimitedDecoder};

pub fn main() {
    let mut input = BytesMut::from(include_str!("../../benches/codecs/moby_dick.txt"));

    let framer = Box::new(CharacterDelimitedDecoder::new(b'a'));
    let deserializer = Box::new(BytesDeserializer::new());
    let mut decoder = Box::new(codecs::Decoder::new(framer, deserializer));

    loop {
        match decoder.decode_eof(&mut input) {
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => {
                unreachable!()
            }
        }
    }
}
