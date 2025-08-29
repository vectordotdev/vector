The `windows_eventlog` source allows collecting logs from Windows Event Log channels using the Windows Event Log API.

This Windows-specific source polls Event Log channels and streams events with comprehensive security validation, configurable field filtering, and support for XPath event queries. Key features include:

- Support for multiple channels (System, Application, Security, etc.)
- XPath query filtering for selective event collection
- Configurable polling intervals and batch sizes
- Bookmark persistence for reliable event tracking
- Security hardening against injection attacks and resource exhaustion
- Flexible event data formatting and field filtering