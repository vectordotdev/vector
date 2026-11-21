The `datadog_agent` source now accepts indexed (`idx`) trace payloads from current Datadog Agents and converts them to the same trace events as the legacy format.

The new advanced `max_trace_expansion_bytes` option, which defaults to 100 MiB, limits how far the indexed trace payloads in a single request can expand during conversion. It counts both strings copied from a payload's string table and numbers, booleans, and ids that become longer tag strings or list elements. Requests that would exceed it are rejected with HTTP 422.

authors: bruceg
