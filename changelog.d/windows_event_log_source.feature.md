The `windows_event_log` source allows collecting logs from Windows Event Log channels using the Windows Event Log API.

This Windows-specific source uses event-driven subscriptions to stream events in real-time with comprehensive security validation, configurable field filtering, and support for XPath event queries. Key features include:

- Support for multiple channels (System, Application, Security, and 140+ specialized channels)
- Real-time event-driven subscriptions using the native Windows Event Log subscription API
- XPath query filtering for selective event collection
- Checkpoint persistence for reliable resumption after restarts (similar to journald)
- End-to-end acknowledgment support for at-least-once delivery guarantees
- Configurable rate limiting to prevent overwhelming downstream systems
- Configurable field truncation for storage optimization
- Enhanced provider name extraction using proper XML parsing
- Support for large events (up to 10MB)
- Security hardening against XPath injection, resource exhaustion, and memory leaks
- Flexible event data formatting and field filtering
- FluentBit-compatible string_inserts field for parameter arrays

New configuration options:
- `events_per_second`: Rate limit event processing (0 = unlimited)
- `max_event_data_length`: Truncate event data values (0 = no truncation)
- `data_dir`: Directory for checkpoint persistence

Note: Wildcard channel patterns (e.g., `Microsoft-Windows-*`) are not supported. Specify exact channel names.

authors: tot19
