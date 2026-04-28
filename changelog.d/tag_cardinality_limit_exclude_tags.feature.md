The `tag_cardinality_limit` transform now accepts an `exclude_tags` option (settable both globally and per-metric) listing tag keys that bypass cardinality limiting entirely. Listed tags pass through unchanged, are not counted against `value_limit`, and never enter the cache. This is useful for tags whose high cardinality is intentional, such as `kube_pod_name` or `tenant_id`. When set on a per-metric configuration, the effective exclusion list is the union of the global and per-metric values.

authors: kaarolch
