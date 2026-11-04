use std::time::Duration;

use metrics::{counter, histogram};
use vector_lib::internal_event::{InternalEvent, error_stage, error_type};
use vector_lib::NamedInternalEvent;

#[derive(Debug, NamedInternalEvent)]
pub struct GcsObjectProcessingSucceeded<'a> {
    pub bucket: &'a str,
    pub duration: Duration,
}

impl InternalEvent for GcsObjectProcessingSucceeded<'_> {
    fn emit(self) {
        debug!(
            message = "GCS object processing succeeded.",
            bucket = %self.bucket,
            duration_ms = %self.duration.as_millis(),
        );
        histogram!(
            "gcs_object_processing_succeeded_duration_seconds",
            "bucket" => self.bucket.to_owned(),
        )
        .record(self.duration);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GcsObjectProcessingFailed<'a> {
    pub bucket: &'a str,
    pub duration: Duration,
}

impl InternalEvent for GcsObjectProcessingFailed<'_> {
    fn emit(self) {
        debug!(
            message = "GCS object processing failed.",
            bucket = %self.bucket,
            duration_ms = %self.duration.as_millis(),
        );
        histogram!(
            "gcs_object_processing_failed_duration_seconds",
            "bucket" => self.bucket.to_owned(),
        )
        .record(self.duration);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GcsPubsubMessageReceiveSucceeded {
    pub count: usize,
}

impl InternalEvent for GcsPubsubMessageReceiveSucceeded {
    fn emit(self) {
        trace!(message = "Received Pub/Sub messages.", count = %self.count);
        counter!("gcs_pubsub_message_receive_succeeded_total").increment(1);
        counter!("gcs_pubsub_message_received_messages_total").increment(self.count as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GcsPubsubMessageReceiveError<'a, E> {
    pub error: &'a E,
}

impl<E: std::fmt::Display> InternalEvent for GcsPubsubMessageReceiveError<'_, E> {
    fn emit(self) {
        error!(
            message = "Failed to pull Pub/Sub messages.",
            error = %self.error,
            error_code = "failed_pulling_pubsub_messages",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "failed_pulling_pubsub_messages",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GcsPubsubMessageProcessingSucceeded<'a> {
    pub message_id: &'a str,
}

impl InternalEvent for GcsPubsubMessageProcessingSucceeded<'_> {
    fn emit(self) {
        trace!(message = "Processed Pub/Sub message successfully.", message_id = %self.message_id);
        counter!("gcs_pubsub_message_processing_succeeded_total").increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GcsPubsubMessageProcessingError<'a, E> {
    pub message_id: &'a str,
    pub error: &'a E,
}

impl<E: std::fmt::Display> InternalEvent for GcsPubsubMessageProcessingError<'_, E> {
    fn emit(self) {
        error!(
            message = "Failed to process Pub/Sub message.",
            message_id = %self.message_id,
            error = %self.error,
            error_code = "failed_processing_pubsub_message",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "failed_processing_pubsub_message",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GcsPubsubMessageAcknowledgeSucceeded {
    pub count: usize,
}

impl InternalEvent for GcsPubsubMessageAcknowledgeSucceeded {
    fn emit(self) {
        trace!(message = "Acknowledged Pub/Sub messages.", count = %self.count);
        counter!("gcs_pubsub_message_acknowledge_succeeded_total").increment(self.count as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GcsPubsubMessageAcknowledgeError<'a, E> {
    pub error: &'a E,
}

impl<E: std::fmt::Display> InternalEvent for GcsPubsubMessageAcknowledgeError<'_, E> {
    fn emit(self) {
        error!(
            message = "Failed to acknowledge Pub/Sub messages.",
            error = %self.error,
            error_code = "failed_acknowledging_pubsub_messages",
            error_type = error_type::ACKNOWLEDGMENT_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "failed_acknowledging_pubsub_messages",
            "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GcsNotificationInvalidEventIgnored<'a> {
    pub bucket: &'a str,
    pub object: &'a str,
    pub event_type: &'a str,
}

impl InternalEvent for GcsNotificationInvalidEventIgnored<'_> {
    fn emit(self) {
        warn!(
            message = "Ignored GCS notification for non-OBJECT_FINALIZE event.",
            bucket = %self.bucket, object = %self.object, event_type = %self.event_type,
        );
        counter!("gcs_notification_ignored_total", "ignore_type" => "invalid_event_type")
            .increment(1);
    }
}
