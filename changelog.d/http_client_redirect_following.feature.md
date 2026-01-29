Added HTTP redirect following support to `prometheus_scrape` and `http_client` sources with configurable `follow_redirects` and `max_redirects` options. Prometheus defaults to following redirects; http_client defaults to not following. The timeout applies to the entire redirect chain, and 301/302/303 redirects automatically change non-HEAD requests to GET (per HTTP spec).

authors: XYenon
