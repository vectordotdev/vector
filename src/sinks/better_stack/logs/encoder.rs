//! Encoding for the `better_stack_logs` sink.

use bytes::BytesMut;
use chrono::{SecondsFormat, Utc};
use serde_json::{json, to_vec};
use std::io;

use crate::sinks::{
    prelude::*,
    util::encoding::{write_all, Encoder as SinkEncoder},
};

pub(super) struct BetterStackLogsEncoder {
    pub(super) transformer: Transformer,
}

impl SinkEncoder<Vec<Event>> for BetterStackLogsEncoder {
    fn encode_input(
        &self,
        events: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();
        let mut body = BytesMut::new();
        let n_events = events.len();

        let mut json_objects = Vec::with_capacity(n_events);

        for mut event in events {
            self.transformer.transform(&mut event);

            byte_size.add_event(&event, event.estimated_json_encoded_size_of());

            let log = event.as_mut_log();

            let timestamp = if let Some(Value::Timestamp(ts)) = log.remove_timestamp() {
                ts
            } else {
                Utc::now()
            };

            log.insert("dt", timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true));

            let json_object = json!(log);
            json_objects.push(json_object);
        }

        let data = to_vec(&json_objects)?;

        let body_content = String::from_utf8_lossy(&data);

        body.extend(body_content.as_bytes());

        let body = body.freeze();

        write_all(writer, n_events, body.as_ref()).map(|()| (body.len(), byte_size))
    }
}
