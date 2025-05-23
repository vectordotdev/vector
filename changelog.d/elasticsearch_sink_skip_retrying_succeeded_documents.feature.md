The `request_retry_partial` behavior for the `elasticsearch` was changed. Now only the failed retriable requests in a bulk will be retried (instead of all requests in the bulk).

authors: Serendo
