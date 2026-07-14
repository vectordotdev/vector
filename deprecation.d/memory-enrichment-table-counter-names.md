---
what: "Legacy memory enrichment table counter metric names"
deprecated_since: "0.57.0"
---

The following memory enrichment table counter metric names are deprecated:

- `memory_enrichment_table_failed_insertions`
- `memory_enrichment_table_failed_reads`
- `memory_enrichment_table_ttl_expirations`

The replacement metrics use the `_total` suffix:

- `memory_enrichment_table_failed_insertions_total`
- `memory_enrichment_table_failed_reads_total`
- `memory_enrichment_table_ttl_expirations_total`

Migrate dashboards, alerts, and other consumers to use the `_total` suffixed metric names.

The deprecated metric names will be removed in a future release.