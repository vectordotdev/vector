The `webhdfs` sink now retries write errors that OpenDAL marks as temporary, such as a failed connection or a `5xx` response, instead of dropping the batch on the first failure. The sink also gains the standard `request` options (`request.retry_attempts`, `request.retry_initial_backoff_secs`, `request.retry_max_duration_secs`, `request.timeout_secs`, `request.concurrency` and the rate limit settings) to tune this behavior.

authors: anpawo
