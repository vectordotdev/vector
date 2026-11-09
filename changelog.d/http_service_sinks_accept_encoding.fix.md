The `clickhouse`, `gcp_stackdriver_logs`, `gcp_stackdriver_metrics`, `greptimedb_logs`, `honeycomb`, and `keep` sinks again send `Accept-Encoding: zstd,gzip,deflate,br` with their event requests and decompress compressed responses, as they did before Vector 0.59.0. From 0.59.0 these sinks sent `Accept-Encoding: identity`. Healthcheck requests are unchanged and still send `Accept-Encoding: identity`.

Response-body transport failures remain retriable for both compressed and uncompressed responses. Malformed compressed responses are not retried.

authors: thomasqueirozb
