The `kafka_consumed_messages_total` and `kafka_consumed_messages_bytes_total` metrics emitted by
the Kafka source now include `topic` and `partition` labels, allowing users to monitor consumption
metrics per topic and partition when multiple topics are configured for a source.

authors: jpds
