HTTP-based sinks using the shared retry logic now classify transport-layer failures with
`HttpError::is_retriable`: connection and TLS connector issues may be retried, while failures
such as invalid HTTP request construction or an invalid proxy URI are not. Setting
`retry_strategy` to `none` disables retries for these transport errors and for request
timeouts, in addition to status-code-based retries.

Issue: https://github.com/vectordotdev/vector/issues/10870

authors: ndrsg
