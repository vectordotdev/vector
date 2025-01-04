//! Encoding for the `keep` sink.

use bytes::Bytes;
use serde_json::{json, to_vec};
use std::io;

use crate::sinks::{
    prelude::*,
    util::encoding::{write_all, Encoder as SinkEncoder},
};

pub(super) struct KeepEncoder {
    pub(super) transformer: Transformer,
}

impl SinkEncoder<Vec<Event>> for KeepEncoder {
    fn encode_input(
        &self,
        events: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();
        let n_events = events.len();
        let mut json_events: Vec<serde_json::Value> = Vec::with_capacity(n_events);

        for mut event in events {
            self.transformer.transform(&mut event);

            byte_size.add_event(&event, event.estimated_json_encoded_size_of());

            let mut data = json!(event.as_log());
            if let Some(message) = data.get("message") {
                if let Some(message_str) = message.as_str() {
                    // Parse the JSON string in `message`
                    let parsed_message: serde_json::Value = serde_json::from_str(message_str)?;

                    // Reassign the parsed JSON back to `message`
                    data["message"] = parsed_message;
                }
            }
            data["keep_source_type"] = json!(event.source_id());

            json_events.push(data);
        }

        let body = Bytes::from(to_vec(&serde_json::Value::Array(json_events))?);

        write_all(writer, n_events, body.as_ref()).map(|()| (body.len(), byte_size))
    }
}
