use std::sync::Arc;

use bytes::Bytes;
use chrono::Utc;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::BytesDeserializer,
    config::LegacyKey,
    internal_event::{CountByteSize, InternalEventHandle as _, emit},
    lookup::path,
};
use warp::{Filter, filters::BoxedFilter, path as warp_path, path::FullPath, reply::Response};

use super::{DatadogAgentConfig, DatadogAgentSource, RequestHandler};
use crate::{event::Event, internal_events::HttpBytesReceived, sources::util::http::capped_body};

pub(super) fn build_warp_filter(
    handler: RequestHandler,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(warp_path!("api" / "v0.2" / "stats" / ..))
        .and(source.validated_api_key_filter())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::header::optional::<String>("content-type"))
        .and(capped_body())
        .and_then({
            move |path: FullPath,
                  api_key: Option<Arc<str>>,
                  content_encoding: Option<String>,
                  content_type: Option<String>,
                  body: Bytes| {
                let events = Ok(build_apm_stats_event(
                    body,
                    api_key,
                    content_encoding,
                    content_type,
                    path.as_str(),
                    &source,
                ));
                handler.clone().handle_request(events, super::APM_STATS)
            }
        })
        .boxed()
}

pub(crate) fn build_apm_stats_event(
    body: Bytes,
    api_key: Option<Arc<str>>,
    content_encoding: Option<String>,
    content_type: Option<String>,
    http_path: &str,
    source: &DatadogAgentSource,
) -> Vec<Event> {
    emit(HttpBytesReceived {
        byte_size: body.len(),
        http_path,
        protocol: source.protocol,
    });
    if body.is_empty() {
        debug!(message = "Empty payload ignored.");
        return Vec::new();
    }

    let mut log = BytesDeserializer.parse_single(body, source.log_namespace);
    // compute EstimatedJsonSizeOf before enrichment
    let event_bytes_received = log.estimated_json_encoded_size_of();
    source.log_namespace.insert_standard_vector_source_metadata(
        &mut log,
        DatadogAgentConfig::NAME,
        Utc::now(),
    );
    if let Some(content_encoding) = content_encoding {
        source.log_namespace.insert_source_metadata(
            DatadogAgentConfig::NAME,
            &mut log,
            Some(LegacyKey::InsertIfEmpty(path!("content_encoding"))),
            path!("content_encoding"),
            content_encoding,
        );
    }
    if let Some(content_type) = content_type {
        source.log_namespace.insert_source_metadata(
            DatadogAgentConfig::NAME,
            &mut log,
            Some(LegacyKey::InsertIfEmpty(path!("content_type"))),
            path!("content_type"),
            content_type,
        );
    }
    if let Some(api_key) = api_key {
        log.metadata_mut().set_datadog_api_key(api_key);
    }

    source
        .events_received
        .emit(CountByteSize(1, event_bytes_received));
    vec![Event::Log(log)]
}
