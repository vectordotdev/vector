The `stopped_total` internal metric has a new `reason` tag that indicates whether an error triggered the shutdown. The value is `none` for a normal shutdown, such as after a shutdown signal or after all sources finish. Otherwise, the value is one of `source_aborted`, `transform_aborted`, `sink_aborted`, `api_failed`, or `reload_failed_to_restore`.

authors: gwenaskell
