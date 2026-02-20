The `throttle` transform now supports multi-dimensional rate limiting with independent
thresholds for event count (`threshold.events`), estimated JSON byte size
(`threshold.json_bytes`), and custom VRL token expressions (`threshold.tokens`).
Events are dropped when any threshold is exceeded. A new `reroute_dropped` option
routes throttled events to a named `dropped` output port instead of discarding them.
New opt-in per-key per-threshold metrics provide tenant-level throttling visibility.
The legacy `threshold: <number>` syntax remains fully backward compatible.

authors: slawomirskowron
