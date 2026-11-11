The `clickhouse`, `gcp_stackdriver_logs`, `gcp_stackdriver_metrics`, `greptimedb_logs`, `honeycomb`, and `keep` sinks again send `Accept-Encoding: zstd,gzip,deflate,br` with their event requests and decompress compressed responses. Healthcheck requests are unchanged and still send `Accept-Encoding: identity`.

`Accept-Encoding` sent with event requests:

| 0.58.0 and earlier | 0.59.0 until this release | This release |
| --- | --- | --- |
| `zstd,gzip,deflate,br` | `identity` | `zstd,gzip,deflate,br` |

Response-body transport failures remain retriable for both compressed and uncompressed responses. Malformed compressed responses are not retried.

authors: thomasqueirozb
