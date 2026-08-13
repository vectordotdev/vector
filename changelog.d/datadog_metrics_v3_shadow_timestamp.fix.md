The `datadog_metrics` sink now resolves the "no timestamp" fallback once per flush instead of once
per metric per encoder.

Metrics from sources that don't set a timestamp (such as `statsd`) had their timestamp filled in
with `Utc::now()` independently by the V2 and the V3 shadow encoder. Because the two payloads are
encoded one after the other, any flush whose encoding straddled a second boundary produced
different timestamps in each payload, which made the intake's V2/V3 comparison report large
numbers of series as present on only one side. It also meant a single flush's points could be
split across two seconds within the V2 payload on its own.

authors: stephenwakely
