Custom auth VRL enrichment (`%field` writes) is now supported by all HTTP-based sources
(`http_server`, `heroku_logs`, `prometheus_pushgateway`, `prometheus_remote_write`), not just
a subset. Enrichment fields are inserted into event metadata under `http_server.<field>` in the
Vector namespace, or into the event body in the legacy namespace, without overwriting existing
fields.

authors: petere-datadog
