//! Encoding for the `honeycomb` sink.

use bytes::Bytes;
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
        let n_events = events.len();
        let mut json_events: Vec<serde_json::Value> = Vec::with_capacity(n_events);

        for mut event in events {
            self.transformer.transform(&mut event);

            byte_size.add_event(&event, event.estimated_json_encoded_size_of());

            let log = event.as_mut_log();

            let timestamp = match log.remove_timestamp() {
                Some(Value::Timestamp(ts)) => ts,
                _ => Utc::now(),
            };

            let data = json!({
                "time": timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true),
                "data": log.convert_to_fields(),
            });

            json_events.push(data);
        }

        let body = Bytes::from(to_vec(&serde_json::Value::Array(json_events))?);

        write_all(writer, n_events, body.as_ref()).map(|()| (body.len(), byte_size))
    }
}
