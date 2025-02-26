Add support for static labels in gcp_stackdriver_logs sink.

    This enhancement enables users to define static labels directly in the
    gcp_stackdriver_logs sink configuration. Static labels are key-value pairs
    that are consistently applied to all log entries sent to Google Cloud Logging,
    improving log organization and filtering capabilities.


Add support for dynamic labels in gcp_stackdriver_logs sink via `labels_key`.

    This enhancement allows Vector to automatically map fields from structured
    log entries to Google Cloud LogEntry labels. When a structured log contains
    fields matching the configured `labels_key`, Vector will populate the
    corresponding labels in the Google Cloud LogEntry, enabling better log
    organization and filtering in Google Cloud Logging.

authors: stackempty
