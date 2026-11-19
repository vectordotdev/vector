The `clickhouse` sink now accepts metric events in addition to logs when using the default `JSONEachRow`-style formats. Metrics are serialized with the JSON codec as flat objects, and users define the ClickHouse table schema to match. Metrics are not supported together with `batch_encoding` (`arrow_stream`).

authors: valerypetrov
