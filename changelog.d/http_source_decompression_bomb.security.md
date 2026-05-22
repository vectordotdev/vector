HTTP-based sources (`http_server`, `prometheus_pushgateway`, `prometheus_remote_write`, `heroku_logs`, `opentelemetry`) now cap decompressed request bodies at 100 MiB. Previously, a single unauthenticated request carrying a compressed payload (e.g. a gzip bomb) could allocate unbounded memory and OOM-kill the Vector process. Decompressed payloads exceeding the cap are rejected with HTTP 413, as are requests whose declared `Content-Length` exceeds the same limit.

authors: pront
