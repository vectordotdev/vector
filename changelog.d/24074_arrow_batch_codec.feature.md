A generic Arrow codec has been added to support Apache Arrow IPC serialization across Vector. This enables sinks like `clickhouse` sink to use the ArrowStream format endpoint with significantly better performance and smaller payload sizes compared to JSON-based formats.

authors: benjamin-awd
