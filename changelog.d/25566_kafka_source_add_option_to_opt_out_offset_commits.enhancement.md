The `kafka` source gains a `commit_offsets` option (default `true`). Setting it to `false` disables both librdkafka's background auto-commit and Vector's explicit synchronous commits at shutdown and rebalance, allowing each consumer instance to replay a topic from offset 0 on every restart.

authors: ronitanilkumar