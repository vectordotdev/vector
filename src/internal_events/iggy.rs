#![allow(dead_code)] // TODO requires optional feature compilation

use vector_lib::{
    NamedInternalEvent, counter, gauge,
    internal_event::{
        ComponentEventsDropped, CounterName, GaugeName, InternalEvent, UNINTENTIONAL, error_stage,
        error_type,
    },
    json_size::JsonSize,
};

#[derive(Debug, NamedInternalEvent)]
pub struct IggyBytesReceived<'a> {
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
            stream = self.stream,
            topic = self.topic,
            partition = %self.partition,
        );
        counter!(
            CounterName::ComponentReceivedBytesTotal,
            "stream" => self.stream.to_string(),
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .increment(self.byte_size as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct IggyEventsReceived<'a> {
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
            "stream" => self.stream.to_string(),
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .increment(self.count as u64);
        counter!(
            CounterName::ComponentReceivedEventBytesTotal,
            "stream" => self.stream.to_string(),
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .increment(self.byte_size.get() as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct IggyOffsetUpdated<'a> {
    pub stream: &'a str,
    pub topic: &'a str,
    pub partition: u32,
    pub message_offset: u64,
    pub current_offset: u64,
}

impl InternalEvent for IggyOffsetUpdated<'_> {
    fn emit(self) {
        let lag = self.current_offset.saturating_sub(self.message_offset);
        gauge!(
            GaugeName::IggyConsumerLagMessages,
            "stream" => self.stream.to_string(),
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .set(lag as f64);
        gauge!(
            GaugeName::IggyConsumerOffset,
            "stream" => self.stream.to_string(),
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .set(self.message_offset as f64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct IggyOffsetCommitted<'a> {
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
        .set(self.offset as f64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct IggyReadError {
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
pub struct IggyOffsetUpdateError {
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

#[derive(Debug, NamedInternalEvent)]
pub struct IggySendError<'a> {
    pub count: usize,
    pub error: &'a iggy::prelude::IggyError,
}

impl InternalEvent for IggySendError<'_> {
    fn emit(self) {
        let reason = "Failed to send messages to Iggy.";
        error!(
            message = reason,
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason,
        });
    }
}
