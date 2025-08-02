The `nats` sink now supports message headers when publishing to JetStream.

It introduces a configurable, templated Nats-Msg-Id header that ensures a unique ID for each message. This enables broker-level deduplication, resulting in stronger delivery guarantees and exactly-once semantics when combined with idempotent consumers.

authors: benjamin-awd
