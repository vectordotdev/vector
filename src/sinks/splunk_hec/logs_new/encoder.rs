use std::io;

use vector_core::{
    config::log_schema,
    event::{Event, LogEvent, Value},
};

use super::{service::Encoding, sink::ProcessedEvent};
use crate::{
    internal_events::{SplunkEventEncodeError, SplunkEventSent},
    sinks::util::encoding::{Encoder, EncodingConfiguration},
};
use serde_json::json;

#[derive(PartialEq, Default, Clone, Debug)]
pub struct HecLogsEncoder {
    pub encoding: Encoding,
}

impl HecLogsEncoder {
    fn encode_event(&self, processed_event: ProcessedEvent) -> Option<Vec<u8>> {
        let log = processed_event.log;

        let event = match self.encoding {
            Encoding::Json => json!(&log),
            Encoding::Text => json!(log
                .get(log_schema().message_key())
                .map(|v| v.to_string_lossy())
                .unwrap_or_else(|| "".into())),
        };

        let mut body = json!({
            "event": event,
            "fields": processed_event.fields,
            "time": processed_event.timestamp
        });

        if let Some(host) = processed_event.host {
            let host = host.to_string_lossy();
            body["host"] = json!(host);
        }

        if let Some(index) = processed_event.index {
            body["index"] = json!(index.as_str());
        }

        if let Some(source) = processed_event.source {
            body["source"] = json!(source.as_str());
        }

        if let Some(sourcetype) = processed_event.sourcetype {
            body["sourcetype"] = json!(sourcetype.as_str());
        }

        match serde_json::to_vec(&body) {
            Ok(value) => {
                emit!(&SplunkEventSent {
                    byte_size: value.len()
                });
                Some(value)
            }
            Err(error) => {
                emit!(&SplunkEventEncodeError { error });
                None
            }
        }
    }
}

impl Encoder<Vec<ProcessedEvent>> for HecLogsEncoder {
    fn encode_input(
        &self,
        input: Vec<ProcessedEvent>,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<usize> {
        let encoded_input: Vec<u8> = input
            .into_iter()
            .filter_map(|e| self.encode_event(e))
            .flatten()
            .collect();
        let encoded_size = encoded_input.len();
        writer.write_all(encoded_input.as_slice())?;
        Ok(encoded_size)
    }
}

impl<E> Encoder<Vec<ProcessedEvent>> for E
where
    E: EncodingConfiguration,
    E::Codec: Encoder<Vec<ProcessedEvent>>,
{
    fn encode_input(
        &self,
        mut input: Vec<ProcessedEvent>,
        writer: &mut dyn io::Write,
    ) -> io::Result<usize> {
        for event in input.iter_mut() {
            self.apply_rules(&mut Event::from(event.log.clone()));
        }
        self.codec().encode_input(input, writer)
    }
}
