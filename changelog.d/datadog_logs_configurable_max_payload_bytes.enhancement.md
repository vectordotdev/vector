Add `max_payload_bytes` configuration option to the `datadog_logs` sink, allowing the
payload size limit to be raised above the default 5 MB for endpoints that accept larger
payloads. The batch goal is derived automatically as `max_payload_bytes - 750,000` bytes,
keeping the same safety headroom as the previous hardcoded defaults.

authors: dd-sebastien-lb
