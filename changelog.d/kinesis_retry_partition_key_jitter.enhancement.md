The `aws_kinesis_streams` sink now re-randomizes the partition key when retrying records that failed due to partial `PutRecords` failures, but only when no explicit `partition_key_field` is configured. This improves retry success rates by distributing retried records across different shards, avoiding repeated throttling on the same shard.

authors: hligit
