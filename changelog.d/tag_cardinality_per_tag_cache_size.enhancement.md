Adds a per-tag `cache_size_per_key` option to configuration options in probabilistic mode. Previously, per-tag overrides always inherited the bloom filter cache size from the enclosing config, which could cause a higher false positive rate when the per-tag `value_limit` is higher than the global or per-metric `value_limit`. When omitted, the cache size value from the enclosing config is used. Only valid in `probabilistic` mode — using it in `exact` mode will cause a configuration error.

authors: ArunPiduguDD
