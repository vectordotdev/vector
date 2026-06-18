Adds support for specifying a `cache_size_per_key` in per-tag override configuration options when in probabilistic mode. Previously, even if a per-tag `value_limit` override is specified, it would still inherit the same `cache_size_per_key` as the enclosing global/per-metric configuration, which can lead to a higher false positive rate if the per-tag `value_limit` is higher than the enclosing global/per-metric `value_limit`. The field is optional and falls back to the enclosing per-metric or global value when omitted (ignored when `mode` is `exact`).

authors: ArunPiduguDD
