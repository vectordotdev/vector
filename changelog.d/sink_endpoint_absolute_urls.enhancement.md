Sink `endpoint` options now require an absolute URL that includes a host. Endpoints without a scheme are defaulted to `https://` (for example `endpoint: "localhost:8080"` becomes `https://localhost:8080`).

Previously, partial or empty endpoints (for example `endpoint: ""` or `endpoint: "localhost:8080"` without a scheme) were accepted at configuration load and only failed when the sink attempted to send data, or were silently completed with a default scheme and host.

Empty, host-less, or non-`http(s)` endpoints (for example `endpoint: ""`, `endpoint: "/path"`, or `endpoint: "ftp://example.com"`) are now rejected at configuration load with a clear error, including with `vector validate --no-environment`.

This affects the `keep` and `new_relic` sinks.

authors: thomasqueirozb
