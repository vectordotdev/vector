The `prometheus_scrape` source now sends `Accept-Encoding: gzip` by default and automatically decompresses gzip responses, matching the behaviour of Prometheus and VictoriaMetrics scrapers. This can be overridden via the new `headers` config option.

authors: mushrowan
