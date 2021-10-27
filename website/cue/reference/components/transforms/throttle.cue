package metadata

components: transforms: throttle: {
	title: "Throttle"

	description: """
		Rate limits one or more log streams to limit load on downstream services, or to enforce usage quotas on users.
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		filter: {}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		exclude: {
			common: true
			description: """
				The set of logical conditions to exclude events from rate limiting.
				"""
			required: false
			warnings: []
			type: string: {
				default: null
				examples: [
					#".status_code != 200 && !includes(["info", "debug"], .severity)"#,
				]
				syntax: "remap_boolean_expression"
			}
		}
		key_field: {
			common: false
			description: """
				The name of the log field whose value will be hashed to determine if the event should be rate limited.

				Each unique key will create a buckets of related events to be rate limited separately. If left unspecified,
				or if the event doesn’t have `key_field`, the event be will not be rate limited separately.
				"""
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["message", "{{ hostname }}"]
				syntax: "template"
			}
		}
		threshold: {
			description: """
				The number of events allowed for a given bucket per configured `window`.

				Each unique key will have its own `threshold`.
				"""
			required: true
			warnings: []
			type: uint: {
				examples: [100, 10000]
				unit: null
			}
		}
		window: {
			description: """
				The time frame in which the configured `threshold` is applied.
				"""
			required: true
			warnings: []
			type: uint: {
				examples: [1, 60, 86400]
				unit: "seconds"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	telemetry: metrics: {
		events_discarded_total: components.sources.internal_metrics.output.metrics.events_discarded_total
	}

	how_it_works: {
		rate_limiting: {
			title: "Rate Limiting"
			body: #"""
				The `throttle` transform will bucket events into rate limiters based on your provided `key_field`, or a
				single bucket if not provided. The rate limiter will allow up to your `threshold` of events through and
				drop any further events for that particular bucket. Any event above the configured rate limit will be
				discarded.
				
				This limit will replenish based on your configured `window` option, such that when the `threshold` has
				been reached it will be fully replenished after the the entire `window` duration has passed. This is
				replenished incrementally throughout the `window` period.
				"""#
		}
	}
}
