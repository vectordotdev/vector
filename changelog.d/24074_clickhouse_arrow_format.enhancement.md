The `clickhouse` sink now supports the `arrow_stream` format option, enabling high-performance binary data transfer using Apache Arrow IPC via Clickhouse's ArrowStream format endpoint. This provides significantly better performance and smaller payload sizes compared to JSON-based formats.

authors: benjamin-awd
