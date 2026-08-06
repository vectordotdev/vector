Added a new `drain` transform that clusters log lines using the Drain log
parsing algorithm and annotates each event with a derived template string
(e.g. `user <*> logged in from <*>`). Mirrors the OpenTelemetry Collector
`drain` processor, including `seed_templates`, `seed_logs`, and
`warmup_min_clusters` for stable templates across deployments. Use the
emitted template field as input to a downstream `filter`/`route` to act on
classes of log patterns.

authors: srstrickland
