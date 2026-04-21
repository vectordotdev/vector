//! Encoding for the `honeycomb` sink.

use std::io;

use bytes::Bytes;
use chrono::{SecondsFormat, Utc};
use serde_json::{json, to_vec};

use vector_lib::lookup::lookup_v2::OptionalTargetPath;

use crate::sinks::{
    prelude::*,
    util::encoding::{Encoder as SinkEncoder, write_all},
};

pub(super) struct HoneycombEncoder {
    pub(super) transformer: Transformer,
    pub(super) samplerate_field: Option<OptionalTargetPath>,
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

            let samplerate = self.samplerate_field.as_ref().and_then(|field| {
                field.path.as_ref().and_then(|path| {
                    log.remove(path).and_then(|value| match value {
                        Value::Integer(rate) if rate > 0 => Some(rate),
                        Value::Integer(rate) => {
                            warn!(
                                message = "Samplerate field value must be a positive integer, ignoring.",
                                field = %path,
                                value = %rate,
                            );
                            None
                        }
                        other => {
                            warn!(
                                message = "Samplerate field value was not an integer, ignoring.",
                                field = %path,
                                value_type = other.kind_str(),
                            );
                            None
                        }
                    })
                })
            });

            let mut data = json!({
                "time": timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true),
                "data": log.convert_to_fields(),
            });

            if let Some(rate) = samplerate {
                data["samplerate"] = json!(rate);
            }

            json_events.push(data);
        }

        let body = Bytes::from(to_vec(&serde_json::Value::Array(json_events))?);

        write_all(writer, n_events, body.as_ref()).map(|()| (body.len(), byte_size))
    }
}
