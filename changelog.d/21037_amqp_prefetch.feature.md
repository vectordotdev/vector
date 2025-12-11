Added a new `prefetch_count` option to the AMQP source configuration. This allows limiting the number of in-flight (unacknowledged) messages per consumer using RabbitMQ's prefetch mechanism (`basic.qos`). Setting this value helps control memory usage and load when processing messages slowly.

authors: elkh510
