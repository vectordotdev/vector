---
date: "2026-XX-XX"
title: "Multi-threshold rate limiting in the `throttle` transform"
description: "Throttle by events, bytes, or custom VRL token cost with per-tenant observability"
authors: ["slawomirskowron"]
pr_numbers: [24702]
release: "0.54.0"
hide_on_release_notes: false
badges:
  type: new feature
  domains: ["transforms"]
---

The `throttle` transform now supports **multi-dimensional rate limiting** with
independent thresholds for event count, estimated JSON byte size, and custom VRL
token expressions. Events are dropped when *any* configured threshold is
exceeded.

## What changed

Previously the `throttle` transform could only rate limit by event count using
a single `threshold` number. This worked well for simple cases but fell short
when downstream services impose per-stream byte rate limits (e.g., Loki's 3
MB/stream limit) or when you need bandwidth-aware throttling on edge devices.

You can now configure up to three independent thresholds:

- **`threshold.events`** — maximum events per window (equivalent to the old
  `threshold` integer)
- **`threshold.json_bytes`** — maximum estimated JSON byte size per window,
  computed via Vector's fast `EstimatedJsonEncodedSizeOf` trait (no
  serialization overhead)
- **`threshold.tokens`** — a VRL expression evaluated per event to produce a
  custom cost (e.g., `strlen(string!(.message))` or `to_int(.cost) ?? 1`)

Each threshold type runs its own GCRA rate limiter. An event is dropped the
moment *any* limiter is exceeded.

## Configuration examples

### Old syntax (still works)

```toml
[transforms.simple]
type = "throttle"
inputs = ["source"]
threshold = 100
window_secs = 60
```

### Loki byte-rate protection

Prevent 429 cascades when services burst large log events past Loki's
per-stream byte limit:

```yaml
transforms:
  loki_guard:
    type: throttle
    inputs: ["app_logs"]
    window_secs: 1
    key_field: "{{ stream }}"
    threshold:
      json_bytes: 3000000   # Match Loki's 3 MB/stream/sec default
```

### Multi-threshold with per-tenant keys

```yaml
transforms:
  per_tenant:
    type: throttle
    inputs: ["source"]
    window_secs: 60
    key_field: "{{ service }}"
    threshold:
      events: 1000
      json_bytes: 500000
      tokens: 'strlen(string!(.message))'
    exclude: '.level == "error"'
```

## Dropped output port

A new `reroute_dropped` option routes throttled events to a named `dropped`
output port instead of silently discarding them. Use this for dead-letter
routing (replay from S3 during off-peak), overflow to cheaper storage tiers,
or audit trails in regulated environments.

```yaml
transforms:
  rate_limit:
    type: throttle
    inputs: ["source"]
    threshold:
      events: 500
    reroute_dropped: true

sinks:
  primary:
    type: loki
    inputs: ["rate_limit"]

  replay_queue:
    type: aws_s3
    inputs: ["rate_limit.dropped"]
    bucket: "my-dead-letter-bucket"
    key_prefix: "throttled/%Y-%m-%d/"
    encoding:
      codec: json
```

## Per-tenant observability

New opt-in metrics provide tenant-level visibility into throttle behavior.
Enable them with `internal_metrics.emit_detailed_metrics: true`:

- `throttle_events_discarded_total` — per-key per-threshold-type discard count
- `throttle_bytes_processed_total` — cumulative estimated JSON bytes per key
- `throttle_tokens_processed_total` — cumulative VRL token cost per key
- `throttle_events_processed_total` — cumulative events per key
- `throttle_utilization_ratio` — current usage / threshold ratio gauge per key

Alert before throttling starts with PromQL:
`throttle_utilization_ratio{threshold_type="json_bytes"} > 0.8`

These metrics are gated behind the opt-in flag to protect against cardinality
explosion when `key_field` produces many unique values.

A bounded-cardinality `throttle_threshold_discarded_total` counter (tagged only
by `threshold_type`, max 3 values) is always emitted with zero overhead.

## Performance impact

Measured overhead relative to events-only baseline (~3.58M events/sec):

| Feature | Overhead | When to use |
|:--------|:--------:|:------------|
| Events-only (existing configs) | **-5% faster** | Free upgrade — SyncTransform rewrite |
| `threshold.json_bytes` | +13% | Loki byte limits, bandwidth-constrained edges |
| `threshold.json_bytes` + `reroute_dropped` | +13-16% | Byte limits + dead-letter routing |
| `threshold.tokens` (VRL) | +74% | Custom cost functions (message length, field-based) |
| All three thresholds | +86% | Maximum rate limiting (still >1.9M events/sec) |
| `emit_detailed_metrics` (100 keys) | +75% | Per-tenant dashboards, utilization alerting |

Key cardinality scales sublinearly: 100x more unique keys causes only
1.25-1.45x slowdown. Memory: ~104 bytes per key per limiter — 10K tenants
with 3 thresholds uses ~3 MB.

## Migration

No migration is needed. The legacy `threshold: <number>` syntax remains fully
backward compatible. The new multi-threshold syntax is additive. Existing
configurations will continue to work without changes.

[throttle]: /docs/reference/configuration/transforms/throttle/
