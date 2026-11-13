The `http_client` and `prometheus_scrape` sources now apply the configured timeout to the whole request, including reading the response body. Before, the timeout only covered sending the request and receiving the response headers, so an endpoint that sent headers and then stalled the body kept the request open indefinitely. Such requests now fail with a timeout error.

authors: JinchengCao
