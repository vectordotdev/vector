Added a new `syslog` sink for sending log events in syslog format (RFC 5424 or RFC 3164) over TCP,
UDP, or Unix stream sockets, with configurable stream framing, TLS support, configurable facility,
severity, app_name, proc_id, and msg_id field mappings, and improved syslog encoder compatibility
for header fields, structured-data names, and common facility/severity aliases.

authors: tot19
