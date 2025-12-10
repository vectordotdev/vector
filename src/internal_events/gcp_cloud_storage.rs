use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct GcsObjectDownloadSucceeded<'a> {
    pub bucket: &'a str,
    pub object: &'a str,
    pub byte_size: usize,
}

impl<'a> InternalEvent for GcsObjectDownloadSucceeded<'a> {
    fn emit(self) {
        debug!(
            message = "Successfully downloaded GCS object.",
            bucket = %self.bucket,
            object = %self.object,
            byte_size = %self.byte_size,
        );
        counter!("component_received_bytes_total")
            .increment(self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct GcsObjectDownloadError<'a> {
    pub bucket: &'a str,
    pub object: &'a str,
    pub error: &'a dyn std::error::Error,
}

impl<'a> InternalEvent for GcsObjectDownloadError<'a> {
    fn emit(self) {
        error!(
            message = "Failed to download GCS object.",
            bucket = %self.bucket,
            object = %self.object,
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct GcsObjectProcessingSucceeded<'a> {
    pub bucket: &'a str,
    pub object: &'a str,
    pub events_count: usize,
}

impl<'a> InternalEvent for GcsObjectProcessingSucceeded<'a> {
    fn emit(self) {
        debug!(
            message = "Successfully processed GCS object.",
            bucket = %self.bucket,
            object = %self.object,
            events_count = %self.events_count,
        );
        counter!("component_received_events_total")
            .increment(self.events_count as u64);
    }
}

#[derive(Debug)]
pub struct GcsObjectProcessingError<'a> {
    pub bucket: &'a str,
    pub object: &'a str,
    pub error: &'a dyn std::error::Error,
}

impl<'a> InternalEvent for GcsObjectProcessingError<'a> {
    fn emit(self) {
        error!(
            message = "Failed to process GCS object.",
            bucket = %self.bucket,
            object = %self.object,
            error = %self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct GcsPubsubMessageReceived<'a> {
    pub subscription: &'a str,
    pub message_count: usize,
}

impl<'a> InternalEvent for GcsPubsubMessageReceived<'a> {
    fn emit(self) {
        debug!(
            message = "Received messages from GCS Pub/Sub subscription.",
            subscription = %self.subscription,
            message_count = %self.message_count,
        );
        counter!("component_received_messages_total")
            .increment(self.message_count as u64);
    }
}

#[derive(Debug)]
pub struct GcsPubsubMessageError<'a> {
    pub subscription: &'a str,
    pub error: &'a dyn std::error::Error,
}

impl<'a> InternalEvent for GcsPubsubMessageError<'a> {
    fn emit(self) {
        error!(
            message = "Error receiving messages from GCS Pub/Sub subscription.",
            subscription = %self.subscription,
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct GcsNotificationReceived<'a> {
    pub bucket: &'a str,
    pub object: &'a str,
    pub event_type: &'a str,
}

impl<'a> InternalEvent for GcsNotificationReceived<'a> {
    fn emit(self) {
        debug!(
            message = "Received GCS bucket notification.",
            bucket = %self.bucket,
            object = %self.object,
            event_type = %self.event_type,
        );
        counter!("gcp_cloud_storage_notifications_received_total")
            .increment(1);
    }
}

#[derive(Debug)]
pub struct GcsNotificationInvalidEventIgnored<'a> {
    pub bucket: &'a str,
    pub object: &'a str,
    pub event_type: &'a str,
}

impl<'a> InternalEvent for GcsNotificationInvalidEventIgnored<'a> {
    fn emit(self) {
        debug!(
            message = "Ignored GCS notification for unsupported event type.",
            bucket = %self.bucket,
            object = %self.object,
            event_type = %self.event_type,
        );
        counter!("gcp_cloud_storage_notifications_ignored_total")
            .increment(1);
    }
}

#[derive(Debug)]
pub struct GcsObjectFilteredByBucket<'a> {
    pub bucket: &'a str,
    pub object: &'a str,
    pub configured_bucket: &'a str,
}

impl<'a> InternalEvent for GcsObjectFilteredByBucket<'a> {
    fn emit(self) {
        debug!(
            message = "Filtered out GCS object due to bucket mismatch.",
            bucket = %self.bucket,
            object = %self.object,
            configured_bucket = %self.configured_bucket,
        );
        counter!("gcp_cloud_storage_objects_filtered_total")
            .increment(1);
    }
}
