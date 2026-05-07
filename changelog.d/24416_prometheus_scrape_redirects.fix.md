The `prometheus_scrape` source (and other HTTP client sources) now follows HTTP redirects (301, 302, 307, 308) instead of treating them as errors.

authors: mushrowan
