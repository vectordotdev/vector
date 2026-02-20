package metadata

components: transforms: throttle: {
	title: "Throttle"

	description: """
		Rate limits one or more log streams by event count, estimated JSON byte size, or custom VRL token cost to limit load on downstream services, or to enforce usage quotas on users. Supports multi-dimensional thresholds and a dropped output port for dead-letter routing.
		"""

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		filter: {}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: generated.components.transforms.throttle.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	output: {
		logs: "": {
			description: "Log events that pass all configured rate limit thresholds."
		}
	}

	telemetry: metrics: {
		events_discarded_total:            components.sources.internal_metrics.output.metrics.events_discarded_total
	}

	examples: [
		{
			title: "Rate limiting by event count"
			input: [
				{
					log: {
						timestamp: "2020-10-07T12:33:21.223543Z"
						message:   "First message"
						host:      "host-1.hostname.com"
					}
				},
				{
					log: {
						timestamp: "2020-10-07T12:33:21.223543Z"
						message:   "Second message"
						host:      "host-1.hostname.com"
					}
				},
			]

			configuration: {
				threshold:   1
				window_secs: 60
			}

			output: [
				{
					log: {
						timestamp: "2020-10-07T12:33:21.223543Z"
						message:   "First message"
						host:      "host-1.hostname.com"
					}
				},
			]
		},
	]

	how_it_works: {
		rate_limiting: {
			title: "Rate Limiting"
			body:  """
				The `throttle` transform will spread load across the configured `window_secs`, ensuring that each bucket's
				throughput averages out to the `threshold` per `window_secs`. It utilizes a [Generic Cell Rate Algorithm](\(urls.gcra)) to
				rate limit the event stream.
				"""
			sub_sections: [
				{
					title: "Buckets"
					body: """
						The `throttle` transform buckets events into rate limiters based on the provided `key_field`, or a
						single bucket if not provided. Each bucket is rate limited separately.
						"""
				},
				{
					title: "Quotas"
					body: """
						Rate limiters use "cells" to determine if there is sufficient capacity for an event to successfully
						pass through a rate limiter. Each event passing through the transform consumes an available cell,
						if there is no available cell the event will be rate limited.

						A rate limiter is created with a maximum number of cells equal to the `threshold`, and cells replenish
						at a rate of `window_secs` divided by `threshold`. For example, a `window_secs` of 60 with a `threshold` of 10
						replenishes a cell every 6 seconds and allows a burst of up to 10 events.
						"""
				},
				{
					title: "Rate Limited Events"
					body: """
						The rate limiter will allow up to `threshold` number of events through and drop any further events
						for that particular bucket when the rate limiter is at capacity. Any event passed when the rate
						limiter is at capacity will be discarded and tracked by an `events_discarded_total` metric tagged
						by the bucket's `key`.
						"""
				},
				{
					title: "Multi-Threshold Behavior"
					body: """
						When multiple threshold types are configured (e.g., `events` and `json_bytes`), each type runs its
						own independent GCRA rate limiter. An event is dropped the moment *any* limiter is exceeded.

						For example, with `threshold.events: 1000` and `threshold.json_bytes: 3000000`, a stream could be
						throttled after 500 events if those events are large enough to consume 3 MB of estimated JSON bytes,
						even though the event count threshold was not reached.
						"""
				},
			]
		}
		byte_based_throttling: {
			title: "Byte-Based Throttling"
			body: """
				When `threshold.json_bytes` is configured, the transform estimates the JSON-encoded size of each event
				using Vector's `EstimatedJsonEncodedSizeOf` trait. This is a fast approximation that avoids actual
				serialization — it recursively estimates the size of each field and value type. The estimated byte
				count is consumed from a separate rate limiter bucket.

				This is useful for controlling costs when downstream services charge by data volume (e.g., cloud
				logging services) or when hitting per-stream byte rate limits (e.g., Loki's 3MB/stream limit).
				"""
			sub_sections: [
				{
					title: "Loki Per-Stream Byte Limits"
					body: """
						Loki enforces a default `per_stream_rate_limit` of 3 MB/s. When a service emits a burst of large
						log events, event-count throttling alone won't prevent 429 rejections — 100 events at 100 KB each
						is 10 MB, far exceeding a 3 MB limit even with a conservative event threshold.

						Configure `threshold.json_bytes` to match the Loki limit:

						```yaml
						transforms:
						  loki_guard:
						    type: throttle
						    inputs: ["app_logs"]
						    window_secs: 1
						    key_field: "{{ stream }}"
						    threshold:
						      json_bytes: 3000000
						```

						This catches byte-rate bursts before they reach Loki, avoiding 429 cascades and the retry storms
						that follow.
						"""
				},
				{
					title: "Edge and IoT Bandwidth Throttling"
					body: """
						On edge devices with limited uplink bandwidth (e.g., satellite links), throttling by bytes ensures
						that a 50-byte heartbeat and a 500 KB firmware diagnostic are not treated equally:

						```yaml
						transforms:
						  bandwidth_guard:
						    type: throttle
						    inputs: ["edge_telemetry"]
						    window_secs: 10
						    key_field: "{{ device_id }}"
						    threshold:
						      json_bytes: 125000
						```

						This caps each device to ~100 Kbps sustained, preventing large diagnostic dumps from starving
						other traffic.
						"""
				},
			]
		}
		custom_token_costs: {
			title: "Custom Token Costs"
			body: """
				When `threshold.tokens` is configured with a VRL expression, the expression is evaluated per event
				to determine a custom cost. The result must be a positive number (integer or float). Non-numeric
				results or errors default to a cost of 1.

				The token cost is consumed from its own rate limiter whose budget is set by the same `threshold.tokens`
				value. This allows flexible cost functions like `strlen(string!(.message))` to throttle by message length,
				or `to_int(.cost) ?? 1` to use a cost field from the event itself.
				"""
		}
		dropped_output: {
			title: "Dropped Output Port"
			body: """
				When `reroute_dropped` is set to `true`, events that exceed any rate limit threshold are sent to
				a named `.dropped` output port instead of being silently discarded. This port can be connected to
				a dead-letter sink (e.g., a file or S3 bucket) for later analysis or replay.

				To connect to the dropped output, use `<transform_name>.dropped` as the input for a downstream
				component.
				"""
			sub_sections: [
				{
					title: "Dead-Letter Queue for Replay"
					body: """
						Route throttled events to S3 or a file sink for later replay during off-peak hours:

						```yaml
						transforms:
						  rate_limit:
						    type: throttle
						    inputs: ["source"]
						    window_secs: 60
						    key_field: "{{ service }}"
						    threshold:
						      events: 1000
						      json_bytes: 3000000
						    reroute_dropped: true

						sinks:
						  primary:
						    type: loki
						    inputs: ["rate_limit"]

						  replay_queue:
						    type: aws_s3
						    inputs: ["rate_limit.dropped"]
						    bucket: "my-dead-letter-bucket"
						    key_prefix: "throttled/{{ service }}/%Y-%m-%d/"
						    encoding:
						      codec: json
						```

						During off-peak windows, replay from S3 back through Vector to recover dropped data — achieving
						zero data loss while maintaining byte-rate compliance.
						"""
				},
				{
					title: "Overflow to Cheaper Storage"
					body: """
						Route excess traffic to a cheaper destination instead of dropping entirely:

						```yaml
						sinks:
						  primary:
						    type: elasticsearch
						    inputs: ["rate_limit"]

						  overflow:
						    type: aws_s3
						    inputs: ["rate_limit.dropped"]
						    bucket: "overflow-logs"
						    encoding:
						      codec: json
						      compression: gzip
						```

						Primary events go to an expensive, fast, indexed store. Overflow goes to budget cold storage
						that can be queried on demand.
						"""
				},
				{
					title: "Audit Trail"
					body: """
						In regulated environments, route dropped events to a local file to prove what was throttled:

						```yaml
						sinks:
						  audit_trail:
						    type: file
						    inputs: ["rate_limit.dropped"]
						    path: "/var/log/vector/audit/throttled/%Y-%m-%d.jsonl"
						    encoding:
						      codec: json
						```

						Events are preserved byte-identical to their input — the throttle transform never modifies events.
						"""
				},
			]
		}
		metrics_observability: {
			title: "Metrics and Observability"
			body: """
				The transform emits several internal metrics for monitoring throttle behavior, organized in three tiers
				with increasing cardinality.
				"""
			sub_sections: [
				{
					title: "Always-On Metrics (Bounded Cardinality)"
					body: """
						These metrics are always emitted regardless of configuration. They are safe for any deployment because
						their cardinality is bounded to a maximum of 4 series.

						- `component_discarded_events_total` — standard Vector component metric for discarded events (1 series).
						- `throttle_threshold_discarded_total` — per-threshold-type discard counter, tagged by `threshold_type`
						  which has at most 3 values: `events`, `json_bytes`, `tokens`.
						"""
				},
				{
					title: "Legacy Per-Key Counter (emit_events_discarded_per_key)"
					body: """
						When `internal_metrics.emit_events_discarded_per_key` is set to `true`, the deprecated
						`events_discarded_total` counter is emitted with a `key` tag. Cardinality scales with the number of
						unique keys. This flag exists for backward compatibility. Performance impact: less than 1%.
						"""
				},
				{
					title: "Detailed Per-Key Metrics (emit_detailed_metrics)"
					body: """
						When `internal_metrics.emit_detailed_metrics` is set to `true`, the following per-key metrics
						are emitted:

						- `throttle_events_discarded_total` — drops per key per threshold type (tags: `key`, `threshold_type`).
						- `throttle_events_processed_total` — total events per key (passed + dropped).
						- `throttle_bytes_processed_total` — cumulative estimated JSON bytes per key.
						- `throttle_tokens_processed_total` — cumulative VRL token cost per key.
						- `throttle_utilization_ratio` — current usage / threshold ratio gauge per key per threshold type (0.0 to 1.0+).

						These metrics enable per-tenant dashboards, proactive alerting (e.g., alert when utilization exceeds 80%
						before throttling starts), and cost attribution based on per-tenant byte volume.

						**Cardinality warning:** Series count scales as `O(unique_keys x threshold_types)`. For 100 keys with
						3 threshold types, this creates approximately 800 metric series. Only enable when `key_field` produces
						a bounded number of values (recommended: fewer than 500 unique keys).
						"""
				},
				{
					title: "Per-Tenant Dashboards"
					body: """
						With detailed metrics enabled and piped to Prometheus via the `internal_metrics` source and
						`prometheus_exporter` sink, you can build Grafana dashboards showing per-service:

						- Event rate vs quota
						- Byte volume vs budget
						- Drop rate by threshold type
						- Utilization heatmap across all tenants

						```yaml
						sources:
						  vector_metrics:
						    type: internal_metrics

						sinks:
						  prometheus:
						    type: prometheus_exporter
						    inputs: ["vector_metrics"]
						```

						Example PromQL queries:

						- Alert before throttling: `throttle_utilization_ratio{threshold_type="json_bytes"} > 0.8`
						- Active throttling detection: `rate(throttle_events_discarded_total[5m]) > 0`
						- Top 10 tenants by byte volume: `topk(10, throttle_bytes_processed_total)`
						- Monthly cost estimate at $0.50/GB: `increase(throttle_bytes_processed_total[30d]) / 1e9 * 0.50`
						"""
				},
			]
		}
		performance_impact: {
			title: "Performance Impact"
			body: """
				Each feature adds overhead independently. The table below summarizes the measured throughput impact
				from Criterion benchmarks (200 samples, 30s measurement, 1024 events/iteration).
				"""
			sub_sections: [
				{
					title: "Overhead by Feature"
					body: """
						The following overhead percentages are measured relative to events-only throttling as the baseline
						(~3.58M events/sec):

						- **Events-only (existing behavior):** Baseline. Existing `threshold: N` configs actually run 5% faster
						  due to the SyncTransform rewrite.
						- **`threshold.json_bytes` only:** +13%. The `EstimatedJsonEncodedSizeOf` trait adds ~36ns/event
						  with no allocation or serialization.
						- **`threshold.events` + `threshold.json_bytes`:** +22%. Two GCRA governor calls plus byte estimation.
						- **`threshold.tokens` (VRL expression):** +74%. VRL `Runtime::resolve()` dominates at ~200ns/event.
						  Expected for interpreted evaluation.
						- **All three thresholds:** +86%. Maximum config. Still processes >1.9M events/sec.
						- **`reroute_dropped`:** +3% on the drop path only. Happy-path (events pass through) has zero overhead.
						- **`emit_detailed_metrics`:** +75% with 100 keys. This comes from updating 3-6 metric counters per event.
						  Only enable where you need tenant-level visibility.

						The typical production config — `json_bytes` with `reroute_dropped` — adds 13-16% overhead.
						"""
				},
				{
					title: "Key Cardinality Scaling"
					body: """
						Throughput scales sublinearly with the number of unique keys due to DashMap's O(1) amortized lookup:

						- 10 keys → 100 keys → 1000 keys causes only 1.25-1.45x slowdown (100x more keys).
						- Memory: ~104 bytes per key per limiter. 10K tenants with 3 thresholds uses ~3 MB.
						- Even 10K keys with all three thresholds and detailed metrics uses under 5 MB — negligible
						  compared to Vector's baseline RSS.
						"""
				},
				{
					title: "When to Enable Detailed Metrics"
					body: """
						- **Fewer than 500 unique keys:** Safe to enable. Bounded cardinality, manageable series count.
						- **500 to 10K keys:** Enable with monitoring. Watch Prometheus scrape times and memory.
						- **More than 10K keys or unbounded keys (e.g., user IDs):** Do not enable. Use the always-on
						  `throttle_threshold_discarded_total` for aggregate visibility instead.
						- **No `key_field` configured:** Low value — only one key, so detailed metrics add just 6 series.
						"""
				},
			]
		}
		backward_compatibility: {
			title: "Backward Compatibility"
			body: """
				The legacy `threshold: <number>` syntax is fully preserved. The `threshold` field uses untagged
				deserialization to accept both the integer form and the new object form.

				Existing configurations will continue to work without any changes. The only observable differences
				for existing configs are:

				- A ~5% throughput improvement (SyncTransform vs the old TaskTransform).
				- One new always-on metric (`throttle_threshold_discarded_total{threshold_type="events"}`) which adds
				  exactly 1 bounded-cardinality series.

				All new features (byte thresholds, token thresholds, dropped output, detailed metrics) are purely
				additive and default to disabled.
				"""
		}
	}
}
