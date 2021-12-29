use std::io;

use serde::{Deserialize, Serialize};
use vector_core::{config::log_schema, event::LogEvent};

use super::sink::HecProcessedEvent;
use crate::{
    internal_events::SplunkEventEncodeError,
    sinks::util::encoding::{Encoder, EncodingConfiguration},
};

#[derive(Serialize, Debug)]
pub enum HecEvent {
    #[serde(rename = "event")]
    Json(LogEvent),
    #[serde(rename = "event")]
    Text(String),
}

#[derive(Serialize, Debug)]
pub struct HecData {
    #[serde(flatten)]
    pub event: HecEvent,
    pub fields: LogEvent,
    pub time: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sourcetype: Option<String>,
}

impl HecData {
    pub const fn new(event: HecEvent, fields: LogEvent, time: f64) -> Self {
        Self {
            event,
            fields,
            time,
            host: None,
            index: None,
            source: None,
            sourcetype: None,
        }
    }
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HecLogsEncoder {
    Json,
    Text,
}

impl Default for HecLogsEncoder {
    fn default() -> Self {
        HecLogsEncoder::Text
    }
}

impl HecLogsEncoder {
    pub fn encode_event(&self, processed_event: HecProcessedEvent) -> Option<Vec<u8>> {
        let log = processed_event.event;
        let metadata = processed_event.metadata;
        let event = match self {
            HecLogsEncoder::Json => HecEvent::Json(log),
            HecLogsEncoder::Text => HecEvent::Text(
                log.get(log_schema().message_key())
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".to_string()),
            ),
        };

        let mut hec_data = HecData::new(event, metadata.fields, metadata.timestamp);
        hec_data.host = metadata.host.map(|host| host.to_string_lossy());
        hec_data.index = metadata.index;
        hec_data.source = metadata.source;
        hec_data.sourcetype = metadata.sourcetype;

        match serde_json::to_vec(&hec_data) {
            Ok(value) => Some(value),
            Err(error) => {
                emit!(&SplunkEventEncodeError { error });
                None
            }
        }
    }
}

impl Encoder<Vec<HecProcessedEvent>> for HecLogsEncoder {
    fn encode_input(
        &self,
        input: Vec<HecProcessedEvent>,
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

impl<E> Encoder<Vec<HecProcessedEvent>> for E
where
    E: EncodingConfiguration,
    E::Codec: Encoder<Vec<HecProcessedEvent>>,
{
    fn encode_input(
        &self,
        mut input: Vec<HecProcessedEvent>,
        writer: &mut dyn io::Write,
    ) -> io::Result<usize> {
        for event in input.iter_mut() {
            self.apply_rules(event);
        }
        self.codec().encode_input(input, writer)
    }
}
