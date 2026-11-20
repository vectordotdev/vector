The `http_server` and `opentelemetry` sources now support optional CORS configuration
through `cors` and `http.cors`, respectively. Configure `allowed_origins`,
`allowed_headers`, and `max_age_secs` to receive data directly from browsers.
Preflight requests do not generate events. CORS remains disabled by default.

authors: titaneric
