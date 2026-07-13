Three memory enrichment table counters now also emit a `_total`-suffixed name to comply
with the instrumentation spec, which requires counters to end in `total`:

- `memory_enrichment_table_failed_insertions` → `memory_enrichment_table_failed_insertions_total`
- `memory_enrichment_table_failed_reads` → `memory_enrichment_table_failed_reads_total`
- `memory_enrichment_table_ttl_expirations` → `memory_enrichment_table_ttl_expirations_total`

The old names are still emitted for now to avoid breaking existing scrape configs and
dashboards, but are deprecated and will be removed in a future release. Consumers should
migrate to the `_total`-suffixed names.

authors: RajMandaliya
