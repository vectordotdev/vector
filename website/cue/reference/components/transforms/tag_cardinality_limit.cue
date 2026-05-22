package metadata

components: transforms: tag_cardinality_limit: {
	title: "Tag Cardinality Limit"

	description: """
		Limits the cardinality of tags on metric events, protecting against
		accidental high cardinality usage that can commonly disrupt the stability
		of metrics storages.

		The default behavior is to drop the tag from incoming metrics when the configured
		limit would be exceeded. Note that this is usually only useful when applied to
		incremental counter metrics and can have unintended effects when applied to other
		metric types. The default action to take can be modified with the
		`limit_exceeded_action` option.
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      true
	}

	features: filter: {}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	// TODO: It'd be nice to have a way to define the description of the enum tag field on the Rust
	// side and propagate it forward, since this is a common pattern that gets used.
	configuration: generated.components.transforms.tag_cardinality_limit.configuration & {
		mode: description: "Controls the approach taken for tracking tag cardinality."
	}

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: false
	}

	output: metrics: "": description: "The modified input `metric` event."

	examples: [
		{
			title: "Drop high-cardinality tag"
			notes: """
				In this example we'll demonstrate how to drop a
				high-cardinality tag named `user_id`. Notice that the
				second metric's `user_id` tag has been removed. That's
				because it exceeded the `value_limit`.
				"""
			configuration: {
				value_limit:           1
				limit_exceeded_action: "drop_tag"
			}
			input: [
				{metric: {
					kind: "incremental"
					name: "logins"
					counter: value: 2.0
					tags: user_id:  "user_id_1"
				}},
				{metric: {
					kind: "incremental"
					name: "logins"
					counter: value: 2.0
					tags: user_id:  "user_id_2"
				}},
			]
			output: [
				{metric: {
					kind: "incremental"
					name: "logins"
					counter: value: 2.0
					tags: user_id:  "user_id_1"
				}},
				{metric: {
					kind: "incremental"
					name: "logins"
					counter: value: 2.0
					tags: {}
				}},
			]
		},
	]

	how_it_works: {
		intended_usage: {
			title: "Intended Usage"
			body: """
				This transform is intended to be used as a protection mechanism to prevent
				upstream mistakes. Such as a developer accidentally adding a `request_id`
				tag. When this is happens, it is recommended to fix the upstream error as soon
				as possible. This is because Vector's cardinality cache is held in memory and it
				will be erased when Vector is restarted. This will cause new tag values to pass
				through until the cardinality limit is reached again. For normal usage this
				should not be a common problem since Vector processes are normally long-lived.
				"""
		}

		memory_utilization: {
			title: "Failed Parsing"
			body: """
				This transform stores in memory a copy of the key for every tag on every metric
				event seen by this transform.  In mode `exact`, a copy of every distinct
				value *for each key* is also kept in memory, until `value_limit` distinct values
				have been seen for a given key, at which point new values for that key will be
				rejected.  So to estimate the memory usage of this transform in mode `exact`
				you can use the following formula:

				```text
				(number of distinct field names in the tags for your metrics * average length of
				the field names for the tags) + (number of distinct field names in the tags of
				your metrics * `value_limit` * average length of the values of tags for your
				metrics)
				```

				In mode `probabilistic`, rather than storing all values seen for each key, each
				distinct key has a bloom filter which can probabilistically determine whether
				a given value has been seen for that key.  The formula for estimating memory
				usage in mode `probabilistic` is:

				```text
				(number of distinct field names in the tags for your metrics * average length of
				the field names for the tags) + (number of distinct field names in the tags of
				-your metrics * `cache_size_per_key`)
				```

				The `cache_size_per_key` option controls the size of the bloom filter used
				for storing the set of acceptable values for any single key. The larger the
				bloom filter the lower the false positive rate, which in our case means the less
				likely we are to allow a new tag value that would otherwise violate a
				configured limit. If you want to know the exact false positive rate for a given
				`cache_size_per_key` and `value_limit`, there are many free on-line bloom filter
				calculators that can answer this. The formula is generally presented in terms of
				'n', 'p', 'k', and 'm' where 'n' is the number of items in the filter
				(`value_limit` in our case), 'p' is the probability of false positives (what we
				want to solve for), 'k' is the number of hash functions used internally, and 'm'
				is the number of bits in the bloom filter. You should be able to provide values
				for just 'n' and 'm' and get back the value for 'p' with an optimal 'k' selected
				for you.   Remember when converting from `value_limit` to the 'm' value to plug
				into the calculator that `value_limit` is in bytes, and 'm' is often presented
				in bits (1/8 of a byte).
				"""
		}

		restarts: {
			title: "Restarts"
			body: """
				This transform's cache is held in memory, and therefore, restarting Vector
				will reset the cache. This means that new values will be passed through until
				the cardinality limit is reached again. See [intended usage](#intended-usage)
				for more info.
				"""
		}

		ttl: {
			title: "TTL (sliding-window cardinality)"
			body: """
				By default, the cardinality cache grows monotonically — every distinct value
				ever seen for a tag occupies a slot under `value_limit` until Vector restarts.
				Setting `ttl_secs` turns the cache into a *sliding window*: any tag value not
				observed within that many seconds is dropped, freeing room for fresh values.

				This is useful when the downstream system bills or pages on a rolling
				unique-series window (e.g. Datadog computes custom-metric cardinality on a
				1-hour p95). A monotonic cache will eventually saturate at `value_limit` and
				start rejecting legitimate new values long after the old ones have aged out
				of the billing window.

				```yaml
				type: tag_cardinality_limit
				value_limit: 500
				mode: probabilistic
				cache_size_per_key: 5120
				ttl_secs: 3600       # match the Datadog billing window
				ttl_generations: 4   # eviction granularity = 15 min
				```

				**Refresh-on-sighting**: every cache hit (not just inserts) extends the
				value's lease. Continuously-observed values stay in the cache indefinitely;
				only values that go silent for longer than `ttl_secs` are evicted.

				**Mode interaction**:

				- `mode: exact` — every value carries a precise last-seen timestamp.
				  Eviction is exact to within roughly `ttl_secs / ttl_generations`
				  (capped at a 1-second minimum to keep sweep cost negligible).
				  `ttl_generations` controls only the sweep cadence in exact mode.
				- `mode: probabilistic` — the underlying bloom filter is split into
				  `ttl_generations` rolling shards. Memory cost rises to
				  `ttl_generations * cache_size_per_key` per (metric, tag-key) pair.
				  Reduce `cache_size_per_key` if you want to keep total memory flat.
				`ttl_generations: 1` produces a tumbling window (everything resets
				  at once every `ttl_secs`), which can be useful for matching a strict
				  billing-window boundary.

				**Per-metric overrides do not inherit**: setting `ttl_secs` inside a
				`per_metric_limits.<name>` block (or leaving it unset there) fully
				replaces the global TTL for that metric — it does not fall back to
				the top-level `ttl_secs`. This mirrors the precedence rules for
				`value_limit`. To share the global TTL on a specific metric, copy the
				value explicitly.

				**Restarts still reset the cache** — see [restarts](#restarts).
				"""
		}

		per_tag_limits: {
			title: "Per-tag overrides"
			body: """
				`per_tag_limits` lets you override the cardinality settings for individual
				tag keys instead of changing the metric-level `value_limit`. It is supported
				at two scopes — the top level (applies to every metric that does not match a
				`per_metric_limits` entry) and inside a `per_metric_limits.<name>` block
				(applies only to that metric).

				Each entry uses one of two `mode` values:

				- `mode: limit_override` — track the tag with its own `value_limit`,
				  independent of the surrounding metric's `value_limit`.
				- `mode: excluded` — bypass cardinality tracking for this tag entirely.
				  Values pass through unchanged on every event, are not counted against any
				  `value_limit`, and are never added to the cache.

				```yaml
				type: tag_cardinality_limit
				value_limit: 500
				mode: exact

				# Applies to every metric that does NOT match a per_metric_limits entry below.
				per_tag_limits:
				  kube_pod_name:
				    # High cardinality is intentional for this tag — never track it.
				    mode: excluded
				  request_id:
				    # Tighten the cap for this tag without lowering the metric-level limit.
				    mode: limit_override
				    value_limit: 50

				per_metric_limits:
				  http_requests_total:
				    value_limit: 1000
				    mode: exact
				    # This metric has its own per-tag rules. The top-level per_tag_limits
				    # above is IGNORED for http_requests_total — `kube_pod_name` on this
				    # metric is therefore tracked against value_limit=1000.
				    per_tag_limits:
				      trace_id:
				        mode: excluded
				```

				Precedence is "nearest wins":

				1. If the metric matches a `per_metric_limits` entry, only that entry's
				   `per_tag_limits` is consulted; the top-level `per_tag_limits` is ignored
				   for that metric. (This mirrors how a per-metric `value_limit` shadows the
				   global `value_limit`.)
				2. Otherwise, the top-level `per_tag_limits` is consulted.
				3. Tags not listed in the applicable `per_tag_limits` fall back to the
				   surrounding metric's `value_limit` (per-metric, or global).
				"""
		}
	}

	telemetry: metrics: {
		tag_cardinality_ttl_expirations_total: components.sources.internal_metrics.output.metrics.tag_cardinality_ttl_expirations_total
		tag_value_limit_exceeded_total:        components.sources.internal_metrics.output.metrics.tag_value_limit_exceeded_total
		value_limit_reached_total:             components.sources.internal_metrics.output.metrics.value_limit_reached_total
	}
}
