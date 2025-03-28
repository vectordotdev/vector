#![allow(unused_imports)]
#![allow(warnings)]
use std::{collections::HashMap, io};

use bytes::BytesMut;
use serde_json::{json, to_vec, Map};
use vector_lib::lookup::lookup_v2::ConfigValuePath;
use vrl::event_path;
use vrl::path::PathPrefix;

use crate::{
    sinks::{prelude::*, util::encoding::Encoder as SinkEncoder},
    template::TemplateRenderingError,
};

#[derive(Clone, Debug)]
pub(super) struct OpentelemetryEncoder {
    transformer: Transformer,
}

impl OpentelemetryEncoder {
    /// Creates a new `OpentelemetryEncoder`.
    pub(super) const fn new(transformer: Transformer) -> Self {
        Self { transformer }
    }

    pub(super) fn encode_event(&self, event: Event) -> Option<serde_json::Value> {
        Some(json!({"a": "b"}))
    }
}

impl SinkEncoder<Vec<Event>> for OpentelemetryEncoder {
    fn encode_input(
        &self,
        events: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();
        let mut n_events = events.len();
        let mut body = BytesMut::new();

        let mut entries = Vec::with_capacity(n_events);
        for event in &events {
            let size = event.estimated_json_encoded_size_of();
            if let Some(data) = self.encode_event(event.clone()) {
                byte_size.add_event(event, size);
                entries.push(data)
            } else {
                // encode_event() emits the `TemplateRenderingError` internal event,
                // which emits an `EventsDropped`, so no need to here.
                n_events -= 1;
            }
        }

        let events = json!({ "entries": entries });

        body.extend(&to_vec(&events)?);

        let body = body.freeze();

        write_all(writer, n_events, body.as_ref()).map(|()| (body.len(), byte_size))
    }
}
