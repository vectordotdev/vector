The `windows_event_log` source collects logs from Windows Event Log channels using the native Windows Event Log API.

This Windows-specific source uses pull-mode subscriptions (EvtSubscribe + EvtNext) to stream events in real-time with back-pressure support, configurable field filtering, and XPath event queries. Key features include:

- Support for multiple channels (System, Application, Security, and 140+ specialized channels)
- Pull-mode subscriptions — no event drops under back pressure
- XPath query filtering for selective event collection
- Bookmark-based checkpoint persistence for reliable resumption after restarts
- End-to-end acknowledgment support for at-least-once delivery guarantees
- Rendered human-readable messages via EvtFormatMessage (matches Event Viewer)
- SID-to-account name resolution via LookupAccountSidW
- Metadata enrichment: task names, opcode names, keyword names via EvtFormatMessage
- Configurable rate limiting to prevent overwhelming downstream systems
- Configurable field truncation for storage optimization
- Event data values kept as strings by default for downstream compatibility
- Per-channel metrics: events read, render errors, subscription status, channel record counts
- Graceful handling of missing/inaccessible channels (warn and skip)
- Support for large events (up to 10MB render buffer)
- Security hardening against XPath injection, resource exhaustion, and memory leaks
- FluentBit-compatible string_inserts field for parameter arrays

Configuration options:
- `channels`: List of Windows Event Log channels to subscribe to
- `event_query`: XPath query for filtering events
- `read_existing_events`: Whether to read historical events on first run
- `batch_size`: Number of events per EvtNext call (1–10000, default 100)
- `event_timeout_ms`: WaitForMultipleObjects timeout (default 5000ms)
- `events_per_second`: Rate limit event processing (0 = unlimited)
- `max_event_data_length`: Truncate event data values (0 = no truncation)
- `max_event_age_secs`: Filter out events older than this age
- `checkpoint_interval_secs`: Periodic checkpoint flush interval (default 5s)
- `render_message`: Enable/disable EvtFormatMessage rendering (default true)
- `include_xml`: Include raw event XML in output
- `event_data_format`: Per-field type coercion (string, integer, float, boolean, auto)
- `field_filter`: Include/exclude specific fields, system fields, event data, user data
- `only_event_ids` / `ignore_event_ids`: Event ID allowlist/blocklist
- `data_dir`: Directory for checkpoint persistence
- `acknowledgements`: Enable end-to-end acknowledgments

Note: Wildcard channel patterns (e.g., `Microsoft-Windows-*`) are not supported. Specify exact channel names.

authors: tot19
