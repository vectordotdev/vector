The `azure_blob` sink no longer drops batches on errors that are safe to retry:

- Connection failures (connection refused or reset, DNS, TLS, timeouts) and credential errors (for example, a failed token refresh) are now retried once the Azure SDK's own retries are exhausted, instead of being dropped.
- `408 Request Timeout` responses are now retried, and `501 Not Implemented` responses are no longer retried, matching the other HTTP-based sinks.
- With the default `blob_type: block` and `blob_append_uuid: true`, an upload that finds its blob already written by an earlier attempt of the same request (for example, after a request timeout) is now reported as delivered instead of as an error.

authors: kacpermuda
