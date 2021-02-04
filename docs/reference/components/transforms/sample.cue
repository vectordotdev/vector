package metadata

components: transforms: sample: {
	title: "Sample"

	description: """
		Samples events at a configurable rate.
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      false
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
		key_field: {
			common: false
			description: """
				The name of the log field whose value will be hashed to determine if the event should be passed.

				Consistently samples the same events. Actual rate of sampling may differ from the configured one if
				values in the field are not uniformly distributed. If left unspecified, or if the event doesn't have
				`key_field`, events will be count rated.
				"""
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["message"]
				syntax: "literal"
			}
		}
		exclude: {
			common: true
			description: """
				The set of logical conditions to exclude events from sampling.
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
		rate: {
			description: """
				The rate at which events will be forwarded, expressed as 1/N. For example,
				`rate = 10` means 1 out of every 10 events will be forwarded and the rest will be dropped.
				"""
			required: true
			warnings: []
			type: uint: {
				examples: [10]
				unit: null
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
}
