---
what: "legacy memory enrichment table counters without the `_total` suffix"
deprecated_since: "0.59.0"
---

The memory enrichment table failure and TTL-expiration counters are now also emitted with the
`_total` suffix, matching Vector's counter naming convention:

- `memory_enrichment_table_failed_insertions_total`
- `memory_enrichment_table_failed_reads_total`
- `memory_enrichment_table_ttl_expirations_total`

The previous names without the `_total` suffix are deprecated and will be removed in a future
release. Migrate any dashboards or alerts that reference:

- `memory_enrichment_table_failed_insertions`
- `memory_enrichment_table_failed_reads`
- `memory_enrichment_table_ttl_expirations`
