The `tag_cardinality_limit` transform now accepts a top-level `per_tag_limits` map,
mirroring the per-metric one: `mode: limit_override` to set a per-tag cap, or
`mode: excluded` to bypass cardinality tracking for that tag on every metric without a
`per_metric_limits` entry.

authors: kaarolch
