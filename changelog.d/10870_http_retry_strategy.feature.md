HTTP-based sinks that use the shared retry helpers now support a `retry_strategy` configuration
option to control which HTTP response codes are retried. The `http` sink also includes a new
example showing how to retry only specific transient status codes.

Issue: https://github.com/vectordotdev/vector/issues/10870

authors: ndrsg
