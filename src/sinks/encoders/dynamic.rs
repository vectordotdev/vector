use super::{json::JsonEncoder, string::StringEncoder, Encoder, EncoderConfig};
use crate::event::Event;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct DynamicEncoderConfig {}

#[typetag::serde(name = "dynamic")]
impl EncoderConfig for DynamicEncoderConfig {
    fn build(&self) -> Box<dyn Encoder + Send> {
        Box::new(DynamicEncoder {
            json: JsonEncoder {},
            string: StringEncoder {},
        })
    }
}

struct DynamicEncoder {
    string: StringEncoder,
    json: JsonEncoder,
}

impl Encoder for DynamicEncoder {
    fn encode(&self, event: Event) -> Bytes {
        if event.as_log().is_structured() {
            self.json.encode(event)
        } else {
            self.string.encode(event)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DynamicEncoderConfig;
    use crate::event::Event;
    use crate::sinks::encoders::EncoderConfig;
    use std::collections::HashMap;

    #[test]
    fn dynamic_encoder_uses_string() {
        let encoder = DynamicEncoderConfig::default().build();
        let event = Event::from("hello world");
        let bytes = encoder.encode(event);
        let msg = String::from_utf8(bytes.to_vec()).unwrap();

        assert_eq!(msg, "hello world".to_string());
    }

    #[test]
    fn dynamic_encoder_uses_json() {
        let encoder = DynamicEncoderConfig::default().build();
        let mut event = Event::from("hello world");

        event
            .as_mut_log()
            .insert_explicit("key".into(), "value".into());

        let bytes = encoder.encode(event);
        let map = serde_json::from_slice::<HashMap<String, String>>(&bytes[..]).unwrap();

        assert_eq!(map["message"], "hello world".to_string());
        assert_eq!(map["key"], "value".to_string());
    }
}
