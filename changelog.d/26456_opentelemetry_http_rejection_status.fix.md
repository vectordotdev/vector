The `opentelemetry` source's OTLP/HTTP endpoints now return client error status codes for requests they can't route, instead of `500 Internal Server Error` with an internal debug message in the body. An unsupported or missing `Content-Type` (for example `application/json`) now returns `415 Unsupported Media Type`, a method other than `POST` returns `405 Method Not Allowed`, and an unknown path returns `404 Not Found`.

authors: Andrew-Hinson
