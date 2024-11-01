//! Encoding for the `honeycomb` sink.

use bytes::BytesMut;
use chrono::{SecondsFormat, Utc};
use serde_json::{json, to_vec};
use std::io;

use crate::sinks::{
    prelude::*,
    util::encoding::{write_all, Encoder as SinkEncoder},
};

pub(super) struct HoneycombEncoder {
    pub(super) transformer: Transformer,
}

impl SinkEncoder<Vec<Event>> for HoneycombEncoder {
    fn encode_input(
        &self,
        events: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();
        let mut body = BytesMut::new();
        let n_events = events.len();

        for mut event in events {
            self.transformer.transform(&mut event);

            byte_size.add_event(&event, event.estimated_json_encoded_size_of());

            let log = event.as_mut_log();

            let timestamp = if let Some(Value::Timestamp(ts)) = log.remove_timestamp() {
                ts
            } else {
                Utc::now()
            };

            let data = to_vec(&json!({
                "time": timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true),
                "data": log.convert_to_fields(),
            }))?;

            body.extend(&data);
        }

        let body = body.freeze();

        write_all(writer, n_events, body.as_ref()).map(|()| (body.len(), byte_size))
    }
}
