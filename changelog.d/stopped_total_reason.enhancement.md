The `stopped_total` internal metric now has a `reason` tag that tells whether an error triggered the shutdown. It is `none` for a normal shutdown, such as on a shutdown signal or once all sources have finished, and otherwise one of `source_aborted`, `transform_aborted`, `sink_aborted`, `api_failed` or `reload_failed_to_restore`.

authors: gwenaskell
