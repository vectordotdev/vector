The `prometheus_remote_write` source no longer rejects an entire write request with a 400 when `skip_nan_values` is enabled and a histogram or summary `_count`/`_bucket` sample carries a NaN value. Remote-write senders use NaN to mark a series stale, so a single disappearing scrape target previously caused every sample batched alongside it to be dropped. These samples are now skipped like other NaN values instead of failing the request. Behavior is unchanged when `skip_nan_values` is disabled.

authors: DeviousCardi
