Fixed a delay in sinks that batch events up to `batch.max_bytes`. When an event did not fit in the current batch and filled the next batch by itself, that next batch was sent only after `batch.timeout_secs` if no more events arrived. The batch is now sent immediately.

This affects the `appsignal`, `aws_kinesis_firehose`, `aws_kinesis_streams`, `azure_logs_ingestion`, `databend`, `databricks_zerobus`, `elasticsearch`, `gcp_stackdriver_logs`, `gcp_stackdriver_metrics`, `greptimedb_metrics`, `honeycomb`, `keep`, `new_relic`, `postgres`, `redis`, `statsd`, and `vector` sinks.

authors: thomasqueirozb
