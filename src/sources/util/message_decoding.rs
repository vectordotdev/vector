use std::iter;

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use tokio_util::codec::Decoder as _;
use vector_lib::codecs::StreamDecodingError;
use vector_lib::internal_event::{
    CountByteSize, EventsReceived, InternalEventHandle as _, Registered,
};
use vector_lib::lookup::{metadata_path, path, PathPrefix};
use vector_lib::{config::LogNamespace, EstimatedJsonEncodedSizeOf};

use crate::{codecs::Decoder, config::log_schema, event::BatchNotifier, event::Event};

pub fn decode_message<'a>(
    mut decoder: Decoder,
    source_type: &'static str,
    message: &[u8],
    timestamp: Option<DateTime<Utc>>,
    batch: &'a Option<BatchNotifier>,
    log_namespace: LogNamespace,
    events_received: &'a Registered<EventsReceived>,
) -> impl Iterator<Item = Event> + 'a {
    let schema = log_schema();

    let mut buffer = BytesMut::with_capacity(message.len());
    buffer.extend_from_slice(message);
    let now = Utc::now();

    iter::from_fn(move || loop {
        break match decoder.decode_eof(&mut buffer) {
            Ok(Some((events, _))) => Some(events.into_iter().map(move |mut event| {
                if let Event::Log(ref mut log) = event {
                    log_namespace.insert_vector_metadata(
                        log,
                        schema.source_type_key(),
                        path!("source_type"),
                        Bytes::from(source_type),
                    );
                    match log_namespace {
                        LogNamespace::Vector => {
                            if let Some(timestamp) = timestamp {
                                log.try_insert(metadata_path!(source_type, "timestamp"), timestamp);
                            }

                            log.insert(metadata_path!("vector", "ingest_timestamp"), now);
                        }
                        LogNamespace::Legacy => {
                            if let Some(timestamp) = timestamp {
                                if let Some(timestamp_key) = schema.timestamp_key() {
                                    log.try_insert((PathPrefix::Event, timestamp_key), timestamp);
                                }
                            }
                        }
                    }
                }
                events_received.emit(CountByteSize(1, event.estimated_json_encoded_size_of()));
                event
            })),
            Err(error) => {
                // Error is logged by `crate::codecs::Decoder`, no further handling
                // is needed here.
                if error.can_continue() {
                    continue;
                }
                None
            }
            Ok(None) => None,
        };
    })
    .flatten()
    .map(move |event| event.with_batch_notifier_option(batch))
}
