use super::{Encoder, EncoderConfig};
use crate::Event;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonEncoderConfig {}

#[typetag::serde(name = "json")]
impl EncoderConfig for JsonEncoderConfig {
    fn build(&self) -> Box<dyn Encoder + Send> {
        Box::new(JsonEncoder {})
    }
}

pub struct JsonEncoder {}

impl Encoder for JsonEncoder {
    fn encode(&self, record: Event) -> Bytes {
        serde_json::to_vec(&record.as_log().all_fields())
            .unwrap()
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::JsonEncoderConfig;
    use crate::buffers::Acker;
    use crate::sinks::tcp::TcpSinkConfig;
    use crate::test_util::{block_on, next_addr, receive};
    use crate::topology::config::SinkConfig;
    use crate::Event;
    use futures::{stream, Sink};
    use serde_json::{self, json, Value};

    #[test]
    fn json_encoder() {
        let out_addr = next_addr();

        let config = TcpSinkConfig {
            address: out_addr.to_string(),
            encoder: Box::new(JsonEncoderConfig {}),
        };

        let (sink, _healthcheck) = config.build(Acker::Null).unwrap();

        let output_lines = receive(&out_addr);

        let mut record1 = Event::new_empty();
        record1
            .as_mut_log()
            .insert_explicit("qwerty".into(), "asdf".into());
        record1
            .as_mut_log()
            .insert_explicit("abcd".into(), "1234".into());

        let mut record2 = Event::new_empty();
        record2
            .as_mut_log()
            .insert_explicit("hello".into(), "goodbye".into());
        record2
            .as_mut_log()
            .insert_implicit("hidden".into(), "secret".into());

        block_on(sink.send_all(stream::iter_ok(vec![record1, record2]))).unwrap();

        let output_lines = output_lines.wait();
        assert_eq!(2, output_lines.len());
        assert_eq!(
            serde_json::from_str::<Value>(&output_lines[0]).unwrap(),
            json!({"qwerty": "asdf", "abcd": "1234"})
        );
        assert_eq!(
            serde_json::from_str::<Value>(&output_lines[1]).unwrap(),
            json!({"hello": "goodbye", "hidden": "secret"})
        );
    }
}
