The `tag_cardinality_limit` transform now accepts an `exclude_tags` option (settable globally
and per-metric) that lets specific tag keys bypass cardinality limiting entirely — useful for
tags whose high cardinality is intentional.

authors: kaarolch
