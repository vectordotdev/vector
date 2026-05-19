Renamed the memory enrichment table failure and TTL-expiration internal metrics to end in `_total`, matching Vector's counter naming convention:

- `memory_enrichment_table_failed_insertions_total`
- `memory_enrichment_table_failed_reads_total`
- `memory_enrichment_table_ttl_expirations_total`

This replaces the previous non-`_total` metric names.

authors: nanookclaw
