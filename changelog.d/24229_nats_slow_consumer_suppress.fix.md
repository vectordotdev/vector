The `nats` source now suppresses high-frequency slow consumer warnings that could generate
millions of logs per minute when subscription capacity was exceeded. These events are now
logged at INFO level with rate limiting and tracked via a `nats_slow_consumer_events_total`
metric.

authors: benjamin-awd
