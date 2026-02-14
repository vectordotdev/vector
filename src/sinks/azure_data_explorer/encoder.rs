//! JSONL encoder for the `azure_data_explorer` sink.
//!
//! Each event is serialized to JSON exactly as it exists at the sink boundary
//! (no wrapping, no added/removed fields). Events are joined with newlines
//! to produce JSON Lines / MultiJSON output.

use std::io;

use crate::sinks::{
    prelude::*,
    util::encoding::{write_all, Encoder as SinkEncoder},
};

pub(super) struct AzureDataExplorerEncoder {
    pub(super) transformer: Transformer,
}

impl SinkEncoder<Vec<Event>> for AzureDataExplorerEncoder {
    fn encode_input(
        &self,
        events: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();
        let n_events = events.len();
        let mut body = Vec::new();

        for (i, mut event) in events.into_iter().enumerate() {
            self.transformer.transform(&mut event);

            byte_size.add_event(&event, event.estimated_json_encoded_size_of());

            // Convert to LogEvent and serialize the inner value directly to JSON.
            // This preserves the original event structure without Vector's internal metadata.
            let log = event.into_log();
            
            // Serialize to JSON
            serde_json::to_writer(&mut body, log.value())?;

            // Newline delimiter between events (MultiJSON / JSONL format).
            if i < n_events - 1 {
                body.push(b'\n');
            }
        }

        // Debug: Log the first 2000 bytes of the payload
        if !body.is_empty() {
            let preview_len = body.len().min(2000);
            let preview = String::from_utf8_lossy(&body[..preview_len]);
            debug!(
                message = "Encoded payload for ADX",
                n_events = n_events,
                total_bytes = body.len(),
                preview = %preview,
            );
        }

        write_all(writer, n_events, &body).map(|()| (body.len(), byte_size))
    }
}
