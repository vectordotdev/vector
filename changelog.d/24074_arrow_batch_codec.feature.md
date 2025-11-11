A generic [Apache Arrow](https://arrow.apache.org/) codec has been added to
support [Arrow IPC](https://arrow.apache.org/docs/format/Columnar.html#ipc-streaming-format) serialization across Vector. This enables sinks
like the `clickhouse` sink to use the ArrowStream format endpoint with significantly better performance and smaller payload sizes compared
to JSON-based formats.

authors: benjamin-awd
