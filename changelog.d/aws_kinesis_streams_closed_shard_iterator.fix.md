The `aws_kinesis_streams` source now follows KCL 3.0 closed-shard handling: parent shards are completed with a `SHARD_END` checkpoint instead of being polled until retention expires, children are not claimed until parents are `SHARD_END` (or have no lease), and children start at `TRIM_HORIZON` rather than `LATEST`. Empty GetRecords on a closed shard finishes the consumer; leftover DynamoDB rows for shards no longer in ListShards are deleted. This prevents CloudWatch `GetRecords.IteratorAgeMilliseconds` from climbing for 24 hours and triggering KEDA mass-scale.

authors: benmali
