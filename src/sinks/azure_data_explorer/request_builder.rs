//! `RequestBuilder` implementation for the `azure_data_explorer` sink.
//!
//! The input to the builder is `(String, Vec<Event>)` where the `String` is the
//! resolved ADX table name (produced by `AdxPartitioner`). The table name is
//! stored in the request context (`HttpRequest<String>`) so the service can use
//! it to build the correct ingest URL or ingestion message per batch.

use std::io;

use bytes::Bytes;

use super::encoder::AzureDataExplorerEncoder;
use crate::sinks::{prelude::*, util::http::HttpRequest};

pub(super) struct AzureDataExplorerRequestBuilder {
    pub(super) encoder: AzureDataExplorerEncoder,
    pub(super) compression: Compression,
}

impl RequestBuilder<(String, Vec<Event>)> for AzureDataExplorerRequestBuilder {
    /// `(table_name, finalizers)`
    type Metadata = (String, EventFinalizers);
    type Events = Vec<Event>;
    type Encoder = AzureDataExplorerEncoder;
    type Payload = Bytes;
    /// The `String` context carries the resolved table name to the service.
    type Request = HttpRequest<String>;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (String, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (table, mut events) = input;
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        ((table, finalizers), builder, events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (table, finalizers) = metadata;
        HttpRequest::new(payload.into_payload(), finalizers, request_metadata, table)
    }
}
