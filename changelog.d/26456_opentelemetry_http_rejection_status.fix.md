The `opentelemetry` source's OTLP/HTTP endpoints now return `415 Unsupported Media Type` when the `Content-Type` is missing or is not `application/x-protobuf`. Other rejected requests, such as a request to an unknown path or with a method other than `POST`, now return the applicable `4xx` status code instead of `500 Internal Server Error`.

authors: Andrew-Hinson thomasqueirozb
