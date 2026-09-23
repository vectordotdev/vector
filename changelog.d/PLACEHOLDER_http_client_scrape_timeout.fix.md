The `scrape_timeout_secs` option of the `prometheus_scrape` and `http_client` sources now covers the whole scrape, including reading the response body. Previously it applied only until the response headers arrived, so an endpoint that answered headers promptly and then stalled mid-body could hold a scrape open indefinitely. Scrapes of one endpoint then overlapped and were emitted in completion order, which could deliver an older counter total after a newer one.

authors: praveen-influx
