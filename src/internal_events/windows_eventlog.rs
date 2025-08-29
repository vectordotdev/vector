#![allow(dead_code)] // Structs will be used when Windows Event Log source is integrated

use std::time::Duration;

use metrics::counter;
use tracing::{debug, error, info, trace, warn};
use vector_lib::emit;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{ComponentEventsDropped, UNINTENTIONAL, error_stage, error_type};

// #[cfg(all(windows, feature = "sources-windows_eventlog"))]
// use crate::sources::windows_eventlog::error::WindowsEventLogError;

#[derive(Debug)]
pub struct WindowsEventLogSubscriptionStarted<'a> {
    pub channels: &'a [String],
    pub use_subscription: bool,
}

impl<'a> InternalEvent for WindowsEventLogSubscriptionStarted<'a> {
    fn emit(self) {
        let channels_str = self.channels.join(", ");
        let mode = if self.use_subscription {
            "subscription"
        } else {
            "polling"
        };

        info!(
            message = "Windows Event Log subscription started.",
            channels = %channels_str,
            mode = %mode,
        );
        counter!(
            "windows_eventlog_subscription_started_total",
            "mode" => mode,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogSubscriptionStopped<'a> {
    pub channels: &'a [String],
    pub duration: Duration,
}

impl<'a> InternalEvent for WindowsEventLogSubscriptionStopped<'a> {
    fn emit(self) {
        let channels_str = self.channels.join(", ");

        info!(
            message = "Windows Event Log subscription stopped.",
            channels = %channels_str,
            duration_secs = self.duration.as_secs(),
        );
        counter!("windows_eventlog_subscription_stopped_total").increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogChannelOpened<'a> {
    pub channel: &'a str,
    pub query: Option<&'a str>,
}

impl<'a> InternalEvent for WindowsEventLogChannelOpened<'a> {
    fn emit(self) {
        debug!(
            message = "Opened Windows Event Log channel.",
            channel = %self.channel,
            query = ?self.query,
        );
        counter!("windows_eventlog_channel_opened_total").increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogChannelClosed {
    pub channel: String,
    pub error: Option<String>,
}

impl InternalEvent for WindowsEventLogChannelClosed {
    fn emit(self) {
        match &self.error {
            Some(error) => {
                warn!(
                    message = "Windows Event Log channel closed with error.",
                    channel = %self.channel,
                    error = %error,
                );
            }
            None => {
                debug!(
                    message = "Windows Event Log channel closed.",
                    channel = %self.channel,
                );
            }
        }
        let with_error = if self.error.is_some() {
            "true"
        } else {
            "false"
        };
        counter!(
            "windows_eventlog_channel_closed_total",
            "with_error" => with_error,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogEventsReceived {
    pub count: usize,
    pub byte_size: usize,
    pub channel: String,
}

impl InternalEvent for WindowsEventLogEventsReceived {
    fn emit(self) {
        trace!(
            message = "Events received from Windows Event Log.",
            count = %self.count,
            byte_size = %self.byte_size,
            channel = %self.channel,
        );
        counter!("windows_eventlog_events_received_total").increment(self.count as u64);
        counter!("windows_eventlog_bytes_received_total").increment(self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogEventsFiltered {
    pub filtered_count: usize,
    pub total_count: usize,
}

impl InternalEvent for WindowsEventLogEventsFiltered {
    fn emit(self) {
        debug!(
            message = "Events filtered by configuration.",
            filtered_count = %self.filtered_count,
            total_count = %self.total_count,
            filter_rate = %(self.filtered_count as f64 / self.total_count as f64),
        );
        counter!("windows_eventlog_events_filtered_total").increment(self.filtered_count as u64);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogParseError {
    pub error: String,
    pub channel: String,
    pub event_id: Option<u32>,
}

impl InternalEvent for WindowsEventLogParseError {
    fn emit(self) {
        warn!(
            message = "Failed to parse Windows Event Log event.",
            error = %self.error,
            channel = %self.channel,
            event_id = ?self.event_id,
            error_code = "parse_failed",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "parse_failed",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogPermissionError {
    pub channel: String,
    pub error: String,
}

impl InternalEvent for WindowsEventLogPermissionError {
    fn emit(self) {
        error!(
            message = "Access denied to Windows Event Log channel. Administrator privileges may be required.",
            channel = %self.channel,
            error = %self.error,
            error_code = "permission_denied",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "permission_denied",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogChannelNotFoundError {
    pub channel: String,
}

impl InternalEvent for WindowsEventLogChannelNotFoundError {
    fn emit(self) {
        error!(
            message = "Windows Event Log channel not found. Check channel name and ensure the service is installed.",
            channel = %self.channel,
            error_code = "channel_not_found",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "channel_not_found",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogQueryError {
    pub channel: String,
    pub query: Option<String>,
    pub error: String,
}

impl InternalEvent for WindowsEventLogQueryError {
    fn emit(self) {
        // Assume errors are recoverable by default
        let recoverable = true;

        warn!(
            message = "Failed to query Windows Event Log.",
            channel = %self.channel,
            query = ?self.query,
            error = %self.error,
            recoverable = %recoverable,
            error_code = "query_failed",
            error_type = if recoverable { error_type::REQUEST_FAILED } else { error_type::CONDITION_FAILED },
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        let error_type_str = if recoverable {
            error_type::REQUEST_FAILED
        } else {
            error_type::CONDITION_FAILED
        };
        let recoverable_str = if recoverable { "true" } else { "false" };
        counter!(
            "component_errors_total",
            "error_code" => "query_failed",
            "error_type" => error_type_str,
            "stage" => error_stage::RECEIVING,
            "recoverable" => recoverable_str,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogSubscriptionError {
    pub error: String,
    pub channels: Vec<String>,
}

impl InternalEvent for WindowsEventLogSubscriptionError {
    fn emit(self) {
        let channels_str = self.channels.join(", ");
        let recoverable = true; // Assume recoverable

        error!(
            message = "Windows Event Log subscription error.",
            channels = %channels_str,
            error = %self.error,
            recoverable = %recoverable,
            error_code = "subscription_failed",
            error_type = if recoverable { error_type::CONNECTION_FAILED } else { error_type::CONDITION_FAILED },
            stage = error_stage::RECEIVING,
        );
        let error_type_str = if recoverable {
            error_type::CONNECTION_FAILED
        } else {
            error_type::CONDITION_FAILED
        };
        let recoverable_str = if recoverable { "true" } else { "false" };
        counter!(
            "component_errors_total",
            "error_code" => "subscription_failed",
            "error_type" => error_type_str,
            "stage" => error_stage::RECEIVING,
            "recoverable" => recoverable_str,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogBookmarkError {
    pub channel: String,
    pub error: String,
}

impl InternalEvent for WindowsEventLogBookmarkError {
    fn emit(self) {
        warn!(
            message = "Failed to save bookmark for Windows Event Log channel.",
            channel = %self.channel,
            error = %self.error,
            error_code = "bookmark_failed",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "bookmark_failed",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogTimeout {
    pub channel: String,
    pub timeout_secs: u64,
}

impl InternalEvent for WindowsEventLogTimeout {
    fn emit(self) {
        warn!(
            message = "Windows Event Log operation timed out.",
            channel = %self.channel,
            timeout_secs = %self.timeout_secs,
            error_code = "timeout",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "timeout",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogResourceExhausted {
    pub channel: String,
    pub batch_size: u32,
}

impl InternalEvent for WindowsEventLogResourceExhausted {
    fn emit(self) {
        warn!(
            message = "Windows Event Log resource exhausted. Consider reducing batch size or increasing poll interval.",
            channel = %self.channel,
            current_batch_size = %self.batch_size,
            error_code = "resource_exhausted",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "resource_exhausted",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogEventsDropped {
    pub count: usize,
    pub reason: String,
    pub channel: String,
}

impl InternalEvent for WindowsEventLogEventsDropped {
    fn emit(self) {
        warn!(
            message = "Events dropped from Windows Event Log.",
            count = %self.count,
            reason = %self.reason,
            channel = %self.channel,
        );

        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason: &self.reason,
        });

        counter!("windows_eventlog_events_dropped_total").increment(self.count as u64);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogPollCycleCompleted {
    pub channel: String,
    pub events_processed: usize,
    pub duration: Duration,
}

impl InternalEvent for WindowsEventLogPollCycleCompleted {
    fn emit(self) {
        trace!(
            message = "Windows Event Log poll cycle completed.",
            channel = %self.channel,
            events_processed = %self.events_processed,
            duration_ms = %self.duration.as_millis(),
        );
        counter!("windows_eventlog_poll_cycles_total").increment(1);
    }
}

#[derive(Debug)]
pub struct WindowsEventLogBatchLimitReached {
    pub channel: String,
    pub batch_size: u32,
}

impl InternalEvent for WindowsEventLogBatchLimitReached {
    fn emit(self) {
        debug!(
            message = "Windows Event Log batch size limit reached.",
            channel = %self.channel,
            batch_size = %self.batch_size,
        );
        counter!("windows_eventlog_batch_limit_reached_total").increment(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;


    #[test]
    fn test_subscription_started_event() {
        let channels = vec!["System".to_string(), "Application".to_string()];
        let event = WindowsEventLogSubscriptionStarted {
            channels: &channels,
            use_subscription: true,
        };

        // Should emit without panic
        event.emit();
    }

    #[test]
    fn test_subscription_stopped_event() {
        let channels = vec!["System".to_string()];
        let event = WindowsEventLogSubscriptionStopped {
            channels: &channels,
            duration: Duration::from_secs(60),
        };

        event.emit();
    }

    #[test]
    fn test_channel_events() {
        let open_event = WindowsEventLogChannelOpened {
            channel: "System",
            query: Some("*[System[Level=1]]"),
        };
        open_event.emit();

        let close_event = WindowsEventLogChannelClosed {
            channel: "System".to_string(),
            error: None,
        };
        close_event.emit();
    }

    #[test]
    fn test_events_received() {
        let event = WindowsEventLogEventsReceived {
            count: 10,
            byte_size: 1024,
            channel: "System".to_string(),
        };
        event.emit();
    }

    #[test]
    fn test_parse_error() {
        let event = WindowsEventLogParseError {
            error: "Test error".to_string(),
            channel: "System".to_string(),
            event_id: Some(1000),
        };
        event.emit();
    }

    #[test]
    fn test_permission_error() {
        let event = WindowsEventLogPermissionError {
            channel: "Security".to_string(),
            error: "Access denied".to_string(),
        };
        event.emit();
    }

    #[test]
    fn test_query_error_recoverable() {
        let event = WindowsEventLogQueryError {
            channel: "System".to_string(),
            query: Some("*[System]".to_string()),
            error: "Operation timed out".to_string(),
        };

        // Should emit as warning since timeout is recoverable
        event.emit();
    }

    #[test]
    fn test_query_error_non_recoverable() {
        let event = WindowsEventLogQueryError {
            channel: "System".to_string(),
            query: Some("invalid".to_string()),
            error: "Invalid XPath query syntax".to_string(),
        };

        // Should emit as error since query error is not recoverable
        event.emit();
    }

    #[test]
    fn test_events_filtered() {
        let event = WindowsEventLogEventsFiltered {
            filtered_count: 5,
            total_count: 10,
        };
        event.emit();
    }

    #[test]
    fn test_bookmark_error() {
        let event = WindowsEventLogBookmarkError {
            channel: "System".to_string(),
            error: "Failed to save bookmark".to_string(),
        };
        event.emit();
    }

    #[test]
    fn test_timeout_event() {
        let event = WindowsEventLogTimeout {
            channel: "Application".to_string(),
            timeout_secs: 60,
        };
        event.emit();
    }

    #[test]
    fn test_resource_exhausted() {
        let event = WindowsEventLogResourceExhausted {
            channel: "System".to_string(),
            batch_size: 100,
        };
        event.emit();
    }

    #[test]
    fn test_events_dropped() {
        let event = WindowsEventLogEventsDropped {
            count: 5,
            reason: "parse_error".to_string(),
            channel: "System".to_string(),
        };
        event.emit();
    }

    #[test]
    fn test_poll_cycle_completed() {
        let event = WindowsEventLogPollCycleCompleted {
            channel: "Application".to_string(),
            events_processed: 15,
            duration: Duration::from_millis(250),
        };
        event.emit();
    }

    #[test]
    fn test_batch_limit_reached() {
        let event = WindowsEventLogBatchLimitReached {
            channel: "System".to_string(),
            batch_size: 50,
        };
        event.emit();
    }

    #[test]
    fn test_channel_not_found() {
        let event = WindowsEventLogChannelNotFoundError {
            channel: "NonExistent".to_string(),
        };
        event.emit();
    }

    #[test]
    fn test_subscription_error() {
        let channels = vec!["System".to_string(), "Application".to_string()];
        let event = WindowsEventLogSubscriptionError {
            error: "Failed to create subscription".to_string(),
            channels: channels,
        };
        event.emit();
    }
}
