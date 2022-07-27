use std::collections::HashMap;

use bytes::Bytes;
use http::HeaderMap;

use crate::event::Event;
use crate::sources::util::{ErrorMessage, HttpSource};

#[derive(Clone)]
pub(crate) struct OpentelemetryHttpServer;

impl OpentelemetryHttpServer {
    fn decode_body(&self, _body: Bytes) -> Result<Vec<Event>, ErrorMessage> {
        todo!()
    }
}

impl HttpSource for OpentelemetryHttpServer {
    fn build_events(
        &self,
        body: Bytes,
        _header_map: HeaderMap,
        _query_parameters: HashMap<String, String>,
        _path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        let events = self.decode_body(body)?;
        Ok(events)
    }
}
