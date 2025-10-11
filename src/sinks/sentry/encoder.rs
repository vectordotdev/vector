//! Encoding for the `sentry` sink.

use sentry::Envelope;
use sentry::protocol::{EnvelopeItem, ItemContainer};
use std::io;

use crate::{
    event::Event,
    internal_events::{SentryEncodingError, SentryEventEncoded, SentryEventTypeError},
    sinks::{
        prelude::*,
        util::encoding::{Encoder, write_all},
    },
};
use vector_lib::config::telemetry;

use super::log_convert::convert_to_sentry_log;

#[derive(Clone)]
pub(super) struct SentryEncoder {
    pub(super) transformer: Transformer,
}

impl SentryEncoder {
    pub(super) const fn new(transformer: Transformer) -> Self {
        Self { transformer }
    }
}

// Implement the encoder trait for our Sentry encoder
impl Encoder<Vec<Event>> for SentryEncoder {
    fn encode_input(
        &self,
        events: Vec<Event>,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<(usize, vector_lib::request_metadata::GroupedCountByteSize)> {
        let mut sentry_logs = Vec::new();
        let mut byte_size = telemetry().create_request_count_byte_size();

        for mut event in events {
            self.transformer.transform(&mut event);
            byte_size.add_event(&event, event.estimated_json_encoded_size_of());

            match event {
                Event::Log(log_event) => {
                    sentry_logs.push(convert_to_sentry_log(&log_event));
                }
                Event::Metric(_) => {
                    emit!(SentryEventTypeError {
                        event_type: "metric".to_string(),
                    });
                }
                Event::Trace(_) => {
                    emit!(SentryEventTypeError {
                        event_type: "trace".to_string(),
                    });
                }
            }
        }

        if sentry_logs.is_empty() {
            return Ok((
                0,
                vector_lib::request_metadata::GroupedCountByteSize::default(),
            ));
        }

        let num_logs = sentry_logs.len();

        // Create envelope with ItemContainer
        let item_container = ItemContainer::Logs(sentry_logs);
        let envelope_item = EnvelopeItem::ItemContainer(item_container);

        // Create envelope with the item
        let mut envelope = Envelope::new();
        envelope.add_item(envelope_item);

        // Serialize the envelope to bytes and write
        let mut envelope_bytes = Vec::new();
        envelope.to_writer(&mut envelope_bytes).map_err(|e| {
            let io_error = io::Error::new(io::ErrorKind::InvalidData, e);
            emit!(SentryEncodingError {
                error: io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Failed to serialize Sentry envelope"
                ),
            });
            io_error
        })?;

        if let Err(e) = write_all(writer, num_logs, &envelope_bytes) {
            emit!(SentryEncodingError {
                error: io::Error::new(e.kind(), format!("Failed to write envelope: {}", e))
            });
            return Err(e);
        }

        // Emit success event
        emit!(SentryEventEncoded {
            byte_size: envelope_bytes.len(),
            log_count: num_logs,
        });

        Ok((envelope_bytes.len(), byte_size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codecs::Transformer;
    use vector_lib::event::{Event, LogEvent, Metric, MetricKind, MetricValue};

    fn create_test_encoder() -> SentryEncoder {
        SentryEncoder::new(Transformer::default())
    }

    fn create_test_log_event() -> Event {
        let mut log = LogEvent::from("test message");
        log.insert("level", "info");
        log.insert("trace_id", "12345678-1234-1234-1234-123456789012");
        log.insert("custom_field", "custom_value");
        Event::Log(log)
    }

    #[test]
    fn test_encode_single_log_event() {
        let encoder = create_test_encoder();
        let event = create_test_log_event();
        let mut writer = Vec::new();

        let result = encoder.encode_input(vec![event], &mut writer);

        assert!(result.is_ok());
        let (bytes_written, byte_size) = result.unwrap();
        assert!(bytes_written > 0);
        assert!(!writer.is_empty());
        assert!(byte_size.size().unwrap().0 > 0);
    }

    #[test]
    fn test_encode_multiple_log_events() {
        let encoder = create_test_encoder();
        let events = vec![
            create_test_log_event(),
            create_test_log_event(),
            create_test_log_event(),
        ];
        let mut writer = Vec::new();

        let result = encoder.encode_input(events, &mut writer);

        assert!(result.is_ok());
        let (bytes_written, byte_size) = result.unwrap();
        assert!(bytes_written > 0);
        assert!(!writer.is_empty());
        assert!(byte_size.size().unwrap().0 > 0);
    }

    #[test]
    fn test_encode_empty_events() {
        let encoder = create_test_encoder();
        let events = vec![];
        let mut writer = Vec::new();

        let result = encoder.encode_input(events, &mut writer);

        assert!(result.is_ok());
        let (bytes_written, byte_size) = result.unwrap();
        assert_eq!(bytes_written, 0);
        assert!(writer.is_empty());
        assert_eq!(byte_size.size().unwrap().0, 0);
    }

    #[test]
    fn test_encode_non_log_events() {
        let encoder = create_test_encoder();
        let metric = Event::Metric(Metric::new(
            "test_counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        ));
        let mut writer = Vec::new();

        let result = encoder.encode_input(vec![metric], &mut writer);

        // Should succeed but write nothing as metrics are skipped
        assert!(result.is_ok());
        let (bytes_written, byte_size) = result.unwrap();
        assert_eq!(bytes_written, 0);
        assert!(writer.is_empty());
        // Since no log events were processed, the encoder returns a default GroupedCountByteSize
        assert_eq!(byte_size.size().unwrap().0, 0);
    }

    #[test]
    fn test_encode_log_with_transformer() {
        let transformer = Transformer::default();
        let encoder = SentryEncoder::new(transformer);
        let event = create_test_log_event();
        let mut writer = Vec::new();

        let result = encoder.encode_input(vec![event], &mut writer);

        assert!(result.is_ok());
        let (bytes_written, byte_size) = result.unwrap();
        assert!(bytes_written > 0);
        assert!(byte_size.size().unwrap().0 > 0);
    }
}
