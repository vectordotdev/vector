Adds a new encoder to the Datadog metrics sink to encode series metrics with v3 of the payload
protocol. Set `series_api_version` to `v3` to submit series to `/api/intake/metrics/v3/series`
using the columnar protobuf format, or leave it at the default `v2` to keep using
`/api/v2/series`.

authors: stephenwakely
