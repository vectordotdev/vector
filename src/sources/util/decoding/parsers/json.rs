use super::Parser;
use crate::{config::log_schema, event::Event};
use bytes::Bytes;
use chrono::Utc;
use std::convert::TryInto;

pub struct JsonParser;

impl Parser for JsonParser {
    fn parse(&self, bytes: Bytes) -> crate::Result<Event> {
        let json: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Error parsing JSON: {:?}", error))?;
        let mut event: Event = json.try_into()?;
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key(), Utc::now());
        Ok(event)
    }
}
