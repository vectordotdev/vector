The `clickhouse` sink can now accept metric events in addition to logs with the JSON formats, when the new `accept_metrics` option is enabled (it is disabled by default, so existing sinks keep receiving only logs). Metrics are serialized with the JSON codec as flat objects, and users define the ClickHouse table schema to match. Metrics are not supported with the `arrow_stream` format.

authors: valerypetrov
