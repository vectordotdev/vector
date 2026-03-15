`kafka` sink/source: The `kafka_consumed_messages_total`, `kafka_consumed_messages_bytes_total`,
`kafka_produced_messages_total`, and `kafka_produced_messages_bytes_total` metrics now include
`topic` and `partition` labels, allowing users to monitor both consumption and production metrics
per topic and partition when multiple topics are configured.

The labels on the non-default `kafka_consumer_lag` metric has also had its labels updated for
consistency.

authors: jpds
