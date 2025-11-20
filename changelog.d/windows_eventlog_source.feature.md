The `windows_eventlog` source collects logs from Windows Event Log channels using the Windows Event Log API.

This Windows-specific source uses event-driven subscriptions via the EvtSubscribe API to stream events in real-time. Key features include:

- Support for multiple channels (System, Application, Security, etc.)
- XPath query filtering for selective event collection
- Event-driven architecture with callback-based delivery
- Configurable batch sizes and event timeouts
- Security hardening against injection attacks and resource exhaustion
- Flexible event data formatting and field filtering