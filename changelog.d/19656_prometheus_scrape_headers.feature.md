Added configurable `headers` option to the `prometheus_scrape` source, allowing custom HTTP request headers on scrape requests. This is useful for scraping endpoints that require specific headers, such as HashiCorp Vault's `Accept: application/openmetrics-text`.

authors: mushrowan
