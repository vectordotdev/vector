use std::{collections::BTreeMap, iter};

use serde::Serialize;
use vector_lib::request_metadata::GroupedCountByteSize;
use vector_lib::{config::telemetry, EstimatedJsonEncodedSizeOf};

use super::sink::HecProcessedEvent;
use crate::{internal_events::SplunkEventEncodeError, sinks::util::encoding::Encoder};

#[derive(Serialize, Debug, PartialEq)]
#[serde(untagged)]
pub enum HecFieldValue<'a> {
    Float(f64),
    Str(&'a str),
}

pub type HecFieldMap<'a> = BTreeMap<&'a str, HecFieldValue<'a>>;

#[derive(Serialize, Debug)]
struct HecData<'a> {
    event: &'static str,
    fields: HecFieldMap<'a>,
    time: f64,
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
    pub const fn new(fields: HecFieldMap<'a>, time: f64) -> Self {
        Self {
            event: "metric",
            fields,
            time,
            host: None,
            index: None,
            source: None,
            sourcetype: None,
        }
    }
}

pub struct HecMetricsEncoder;

impl HecMetricsEncoder {
    pub fn encode_event(processed_event: HecProcessedEvent) -> Option<Vec<u8>> {
        let metadata = processed_event.metadata;
        let metric = processed_event.event;

        let fields = metric
            .tags()
            .into_iter()
            .flat_map(|tags| tags.iter_single())
            // skip the metric tags used for templating
            .filter(|(k, _)| !metadata.templated_field_keys.iter().any(|f| f == k))
            .map(|(k, v)| (k, HecFieldValue::Str(v)))
            .chain(iter::once((
                "metric_name",
                HecFieldValue::Str(metadata.metric_name.as_str()),
            )))
            .chain(iter::once((
                "_value",
                HecFieldValue::Float(metadata.metric_value),
            )))
            .collect::<HecFieldMap>();
        let time = metric
            .timestamp()
            .unwrap_or_else(chrono::Utc::now)
            .timestamp_millis() as f64
            / 1000f64;
        let mut hec_data = HecData::new(fields, time);

        hec_data.host = metadata.host;
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

impl Encoder<Vec<HecProcessedEvent>> for HecMetricsEncoder {
    fn encode_input(
        &self,
        input: Vec<HecProcessedEvent>,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();
        for event in &input {
            byte_size.add_event(event, event.estimated_json_encoded_size_of());
        }

        let encoded_input: Vec<u8> = input
            .into_iter()
            .filter_map(Self::encode_event)
            .flatten()
            .collect();
        let encoded_size = encoded_input.len();
        writer.write_all(encoded_input.as_slice())?;
        Ok((encoded_size, byte_size))
    }
}
