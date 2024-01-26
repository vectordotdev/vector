use std::borrow::Cow;

use bytes::BytesMut;
use serde::Serialize;
use tokio_util::codec::Encoder as _;
use vector_lib::request_metadata::GroupedCountByteSize;
use vector_lib::{config::telemetry, EstimatedJsonEncodedSizeOf};

use super::sink::HecProcessedEvent;
use crate::{
    codecs::Transformer,
    event::{Event, LogEvent},
    internal_events::SplunkEventEncodeError,
    sinks::{splunk_hec::common::EndpointTarget, util::encoding::Encoder},
};

#[derive(Serialize, Debug)]
pub enum HecEvent<'a> {
    #[serde(rename = "event")]
    Json(serde_json::Value),
    #[serde(rename = "event")]
    Text(Cow<'a, str>),
}

#[derive(Serialize, Debug)]
pub struct HecData<'a> {
    #[serde(flatten)]
    pub event: HecEvent<'a>,
    pub fields: LogEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sourcetype: Option<String>,
}

impl<'a> HecData<'a> {
    pub const fn new(event: HecEvent<'a>, fields: LogEvent, time: Option<f64>) -> Self {
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

#[derive(Debug, Clone)]
pub struct HecLogsEncoder {
    pub transformer: Transformer,
    pub encoder: crate::codecs::Encoder<()>,
    pub auto_extract_timestamp: bool,
}

impl Encoder<Vec<HecProcessedEvent>> for HecLogsEncoder {
    fn encode_input(
        &self,
        input: Vec<HecProcessedEvent>,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<(usize, GroupedCountByteSize)> {
        let mut encoder = self.encoder.clone();
        let mut byte_size = telemetry().create_request_count_byte_size();
        let encoded_input: Vec<u8> = input
            .into_iter()
            .filter_map(|processed_event| {
                let mut event = Event::from(processed_event.event);
                let metadata = processed_event.metadata;
                self.transformer.transform(&mut event);

                byte_size.add_event(&event, event.estimated_json_encoded_size_of());

                let mut bytes = BytesMut::new();

                match metadata.endpoint_target {
                    EndpointTarget::Raw => {
                        encoder.encode(event, &mut bytes).ok()?;
                        Some(bytes.to_vec())
                    }
                    EndpointTarget::Event => {
                        let serializer = encoder.serializer();
                        let hec_event = if serializer.supports_json() {
                            HecEvent::Json(
                                serializer
                                    .to_json_value(event)
                                    .map_err(|error| emit!(SplunkEventEncodeError { error }))
                                    .ok()?,
                            )
                        } else {
                            encoder.encode(event, &mut bytes).ok()?;
                            HecEvent::Text(String::from_utf8_lossy(&bytes))
                        };

                        let mut hec_data = HecData::new(
                            hec_event,
                            metadata.fields,
                            // If auto_extract_timestamp is set we don't want to pass the timestamp in the
                            // event since we want Splunk to extract it from the message.
                            if self.auto_extract_timestamp {
                                None
                            } else {
                                metadata.timestamp
                            },
                        );
                        hec_data.host = metadata
                            .host
                            .map(|host| host.to_string_lossy().into_owned());
                        hec_data.index = metadata.index;
                        hec_data.source = metadata.source;
                        hec_data.sourcetype = metadata.sourcetype;

                        match serde_json::to_vec(&hec_data) {
                            Ok(value) => Some(value),
                            Err(error) => {
                                emit!(SplunkEventEncodeError {
                                    error: error.into()
                                });
                                None
                            }
                        }
                    }
                }
            })
            .flatten()
            .collect();

        let encoded_size = encoded_input.len();
        writer.write_all(encoded_input.as_slice())?;
        Ok((encoded_size, byte_size))
    }
}
