#![allow(dead_code)] // TODO requires optional feature compilation

use vector_lib::{
    NamedInternalEvent, counter, gauge,
    internal_event::{
        ComponentEventsDropped, CounterName, GaugeName, InternalEvent, LossyF64, UNINTENTIONAL,
        emit, error_stage, error_type,
    },
    json_size::JsonSize,
};

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct IggyBytesReceived<'a> {
    pub byte_size: usize,
    pub stream: &'a str,
    pub topic: &'a str,
    pub partition: u32,
}

impl InternalEvent for IggyBytesReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = "iggy",
            stream = self.stream,
            topic = self.topic,
            partition = %self.partition,
        );
        counter!(
            CounterName::ComponentReceivedBytesTotal,
            "protocol" => "iggy",
            "stream" => self.stream.to_string(),
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .increment(self.byte_size as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct IggyEventsReceived<'a> {
    pub byte_size: JsonSize,
    pub count: usize,
    pub stream: &'a str,
    pub topic: &'a str,
    pub partition: u32,
}

impl InternalEvent for IggyEventsReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = %self.count,
            byte_size = %self.byte_size,
            stream = self.stream,
            topic = self.topic,
            partition = %self.partition,
        );
        counter!(
            CounterName::ComponentReceivedEventsTotal,
            "protocol" => "iggy",
            "stream" => self.stream.to_string(),
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .increment(self.count as u64);
        counter!(
            CounterName::ComponentReceivedEventBytesTotal,
            "protocol" => "iggy",
            "stream" => self.stream.to_string(),
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .increment(self.byte_size.get() as u64);
    }
}

/// Emitted when an Iggy message is polled from the broker, before decoding
/// or delivery. The `iggy_consumer_polled_offset` gauge tracks the polled
/// offset, not the delivered or committed one; see `IggyOffsetCommitted`
/// for the committed offset gauge (`iggy_consumer_committed_offset`).
#[derive(Debug, NamedInternalEvent)]
pub(crate) struct IggyOffsetPolled<'a> {
    pub stream: &'a str,
    pub topic: &'a str,
    pub partition: u32,
    pub message_offset: u64,
}

impl InternalEvent for IggyOffsetPolled<'_> {
    fn emit(self) {
        gauge!(
            GaugeName::IggyConsumerPolledOffset,
            "stream" => self.stream.to_string(),
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .set(LossyF64(self.message_offset));
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct IggyOffsetCommitted<'a> {
    pub stream: &'a str,
    pub topic: &'a str,
    pub partition: u32,
    pub offset: u64,
}

impl InternalEvent for IggyOffsetCommitted<'_> {
    fn emit(self) {
        gauge!(
            GaugeName::IggyConsumerCommittedOffset,
            "stream" => self.stream.to_string(),
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .set(LossyF64(self.offset));
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct IggyReadError {
    pub error: iggy::prelude::IggyError,
}

impl InternalEvent for IggyReadError {
    fn emit(self) {
        error!(
            message = "Failed to read message.",
            error = %self.error,
            error_code = "reading_message",
            error_type = error_type::READER_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "reading_message",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct IggyConsumerStreamEndedError;

impl InternalEvent for IggyConsumerStreamEndedError {
    fn emit(self) {
        error!(
            message = "Iggy consumer stream ended unexpectedly. The SDK's reconnection logic gave up; the source will stop.",
            error_code = "consumer_stream_ended",
            error_type = error_type::READER_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "consumer_stream_ended",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct IggyInvalidPayloadError {
    pub payload_len: usize,
}

impl InternalEvent for IggyInvalidPayloadError {
    fn emit(self) {
        let reason = "Encoded event payload is outside the Iggy SDK's accepted size range.";
        error!(
            message = reason,
            payload_len = self.payload_len as u64,
            max_payload_size = iggy::prelude::MAX_PAYLOAD_SIZE,
            error_code = "invalid_payload",
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "invalid_payload",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        emit(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct IggyOffsetUpdateError {
    pub error: iggy::prelude::IggyError,
}

impl InternalEvent for IggyOffsetUpdateError {
    fn emit(self) {
        error!(
            message = "Unable to update consumer offset.",
            error = %self.error,
            error_code = "iggy_offset_update",
            error_type = error_type::READER_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "iggy_offset_update",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }
}

/// Emitted when the consumer cannot be initialized. The source retries with
/// backoff, so a persistent failure shows up here and nowhere else.
#[derive(Debug, NamedInternalEvent)]
pub(crate) struct IggyConsumerInitError;

impl InternalEvent for IggyConsumerInitError {
    fn emit(self) {
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "consumer_init_failed",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

/// Emitted once per sink request the retry layer gave up on.
///
/// The driver finalizes those as `Errored`, which it only logs: it suppresses
/// `CallError` for anything but `Rejected`, so without this event an outage
/// never reaches `component_errors_total`.
#[derive(Debug, NamedInternalEvent)]
pub(crate) struct IggySinkRequestFailed;

impl InternalEvent for IggySinkRequestFailed {
    fn emit(self) {
        // No `ComponentEventsDropped`: the request is retried upstream and the
        // driver already logs the failure.
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "sink_request_failed",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }
}
