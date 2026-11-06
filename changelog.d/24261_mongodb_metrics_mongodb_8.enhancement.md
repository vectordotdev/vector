The `mongodb_metrics` source now supports MongoDB 8.0 and newer servers. On these servers, the
`mongod_wiredtiger_concurrent_transactions_*` metrics come from `queues.execution`, the checkpoint metrics
come from `wiredTiger.checkpoint`, and the `mongod_metrics_repl_buffer_*` metrics are the sum of the oplog
write and apply buffers.

authors: thomasqueirozb
