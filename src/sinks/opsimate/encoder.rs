//! Encoding for the `opsimate` sink.

use std::io;

use bytes::Bytes;
use serde_json::to_vec;

use crate::sinks::{
    prelude::*,
    util::encoding::{Encoder as SinkEncoder, write_all},
};

pub(super) struct OpsiMateEncoder {
    pub(super) transformer: Transformer,
}

impl SinkEncoder<Vec<Event>> for OpsiMateEncoder {
    /// Encodes a batch of exactly one event as a single JSON object.
    ///
    /// OpsiMate's custom alerts endpoint accepts one alert object per request, never an array,
    /// so a batch holding anything other than one event is rejected rather than silently
    /// serialized into a shape the server would refuse.
    fn encode_input(
        &self,
        events: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut events = events.into_iter();
        let (Some(mut event), None) = (events.next(), events.next()) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the `opsimate` sink sends exactly one event per request",
            ));
        };

        let mut byte_size = telemetry().create_request_count_byte_size();

        self.transformer.transform(&mut event);
        byte_size.add_event(&event, event.estimated_json_encoded_size_of());

        let body = Bytes::from(to_vec(event.as_log())?);

        write_all(writer, 1, body.as_ref()).map(|()| (body.len(), byte_size))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn alert_event() -> Event {
        let mut log = LogEvent::default();
        log.insert(vrl::event_path!("id"), "disk-full-web-1");
        log.insert(vrl::event_path!("alertName"), "DiskFull");
        log.insert(vrl::event_path!("severity"), "critical");
        log.insert(
            vrl::event_path!("tags"),
            Value::Object(
                [
                    ("service".into(), "web".into()),
                    ("resource".into(), "disk".into()),
                ]
                .into_iter()
                .collect(),
            ),
        );
        Event::Log(log)
    }

    #[test]
    fn one_event_becomes_one_json_object() {
        let encoder = OpsiMateEncoder {
            transformer: Transformer::default(),
        };
        let mut body = Vec::new();

        let (written, _) = encoder
            .encode_input(vec![alert_event()], &mut body)
            .expect("a single event must encode");

        assert_eq!(written, body.len());
        let decoded: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            decoded.is_object(),
            "body must be a single object: {decoded}"
        );
        assert_eq!(
            decoded,
            json!({
                "id": "disk-full-web-1",
                "alertName": "DiskFull",
                "severity": "critical",
                "tags": { "service": "web", "resource": "disk" },
            })
        );
    }

    #[test]
    fn transformer_shapes_the_body() {
        let transformer: Transformer = serde_yaml::from_str(
            r#"
            only_fields: ["id", "alertName", "tags"]
            "#,
        )
        .unwrap();
        let encoder = OpsiMateEncoder { transformer };
        let mut body = Vec::new();

        encoder
            .encode_input(vec![alert_event()], &mut body)
            .unwrap();

        let decoded: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            decoded,
            json!({
                "id": "disk-full-web-1",
                "alertName": "DiskFull",
                "tags": { "service": "web", "resource": "disk" },
            })
        );
    }

    #[test]
    fn more_than_one_event_is_rejected() {
        let encoder = OpsiMateEncoder {
            transformer: Transformer::default(),
        };
        let mut body = Vec::new();

        let err = encoder
            .encode_input(vec![alert_event(), alert_event()], &mut body)
            .expect_err("two events must never be encoded into one request");

        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(body.is_empty(), "nothing may be written on rejection");
    }

    #[test]
    fn empty_batch_is_rejected() {
        let encoder = OpsiMateEncoder {
            transformer: Transformer::default(),
        };
        let mut body = Vec::new();

        let err = encoder
            .encode_input(Vec::new(), &mut body)
            .expect_err("an empty batch has nothing to send");

        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}
