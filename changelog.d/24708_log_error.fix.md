The `opentelemetry` source now logs an error if it fails to start up or during runtime.
This can happen when the configuration is invalid, for example trying to bind to the wrong
IP or when hitting the open file limit.

authors: fbs
