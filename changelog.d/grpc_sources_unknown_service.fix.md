The `vector` and `opentelemetry` sources now answer gRPC requests for unregistered services with
gRPC status `UNIMPLEMENTED` instead of HTTP 404. These requests no longer log an error or
increment `component_errors_total`.

authors: thomasqueirozb
