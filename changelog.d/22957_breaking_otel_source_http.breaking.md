The HTTP mode of the `opentelemetry` source now implements more HTTP status codes to indicate the cause of the error. This makes it easier to identify client side misconfiguration.
Previously it would use the HTTP 500 status code for all errors.

authors: fbs
