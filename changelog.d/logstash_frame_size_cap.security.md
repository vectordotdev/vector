The `logstash` source now rejects frames larger than the configured maximum instead of buffering them indefinitely; previously, a sender could declare an extremely large frame and force the source to hold its bytes in memory until it ran out of memory. The limit is controlled by the existing `--max-decompressed-size-bytes` option (default 100 MiB).

authors: pront
