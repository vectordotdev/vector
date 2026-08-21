Fixed `parse_syslog` and syslog codec parsing for network device messages that use year-first RFC3164-like timestamps, comma-separated `YYYY-MM-DD,HH:MM:SS` timestamps, leap-day RFC3164 timestamps without a year, PRI-only messages without timestamps, multi-line message bodies, or NUL-padded frames, while preserving RFC3339, RFC3164, and RFC5424 parsing behavior.

authors: vitalvas
