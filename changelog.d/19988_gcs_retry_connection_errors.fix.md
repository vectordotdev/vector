The `gcp_cloud_storage` sink now retries requests that fail with connection-level errors, such as a connection reset, instead of dropping them. The number of retries is bounded by the existing `request.retry_attempts` setting.

authors: yoshi9930
