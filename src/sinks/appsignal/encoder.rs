use serde_json::{Value, json};
use vector_lib::request_metadata::GroupedCountByteSize;
use vector_lib::{EstimatedJsonEncodedSizeOf, config::telemetry, event::Event};

use crate::{
    codecs::Transformer,
    sinks::util::encoding::{Encoder, as_tracked_write},
};

#[derive(Clone)]
pub(super) struct AppsignalEncoder {
    pub transformer: Transformer,
}

impl Encoder<Vec<Event>> for AppsignalEncoder {
    fn encode_input(
        &self,
        events: Vec<Event>,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<(usize, GroupedCountByteSize)> {
        let mut result = Value::Array(Vec::new());
        let mut byte_size = telemetry().create_request_count_byte_size();
        for mut event in events {
            self.transformer.transform(&mut event);

            byte_size.add_event(&event, event.estimated_json_encoded_size_of());

            let json = match event {
                Event::Log(log) => json!({ "log": log }),
                Event::Metric(metric) => json!({ "metric": metric }),
                _ => {
                    return Err(std::io::Error::other(format!(
                        "The AppSignal sink does not support this type of event: {event:?}"
                    )));
                }
            };
            if let Value::Array(ref mut array) = result {
                array.push(json);
            }
        }
        let written_bytes =
            as_tracked_write::<_, _, std::io::Error>(writer, &result, |writer, value| {
                serde_json::to_writer(writer, value)?;
                Ok(())
            })?;

        Ok((written_bytes, byte_size))
    }
}
