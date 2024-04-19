The `clickhouse` sink now has a new configuration option, `insert_random_shard`, to tell Clickhouse to insert into a random shard (by setting `insert_distributed_one_random_shard`). See the Clickhouse [Distributed Table Engine docs](https://clickhouse.com/docs/en/engines/table-engines/special/distributed) for details.

authors: rguleryuz
