use super::{Encoder, EncoderConfig};
use crate::event::{self, Event};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct StringEncoderConfig {}

#[typetag::serde(name = "string")]
impl EncoderConfig for StringEncoderConfig {
    fn build(&self) -> Box<dyn Encoder + Send> {
        Box::new(StringEncoder {})
    }
}

struct StringEncoder {}

impl Encoder for StringEncoder {
    fn encode(&self, record: Event) -> Bytes {
        record.into_value(&event::MESSAGE).unwrap().into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::StringEncoderConfig;
    use crate::buffers::Acker;
    use crate::event::{self, Event};
    use crate::sinks::tcp::TcpSinkConfig;
    use crate::test_util::{block_on, next_addr, receive};
    use crate::topology::config::SinkConfig;
    use futures::{stream, Sink};

    #[test]
    fn string_encoder() {
        let out_addr = next_addr();

        let config = TcpSinkConfig {
            address: out_addr.to_string(),
            encoder: Box::new(StringEncoderConfig {}),
        };

        let (sink, _healthcheck) = config.build(Acker::Null).unwrap();

        let output_lines = receive(&out_addr);

        let mut record1 = Event::new_empty();
        record1.insert_explicit(event::MESSAGE.clone(), "this is the message".into());
        record1.insert_explicit("abcd".into(), "1234".into());

        let mut record2 = Event::new_empty();
        record2.insert_explicit("hello".into(), "goodbye".into());
        record2.insert_implicit(event::MESSAGE.clone(), "pssst".into());

        block_on(sink.send_all(stream::iter_ok(vec![record1, record2]))).unwrap();

        let output_lines = output_lines.wait();
        assert_eq!(2, output_lines.len());
        assert_eq!(output_lines[0], "this is the message");
        assert_eq!(output_lines[1], "pssst");
    }
}
