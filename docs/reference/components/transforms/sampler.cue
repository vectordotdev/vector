package metadata

components: transforms: sampler: {
	title: "Sampler"

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
	}

	features: {
		filter: {}
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		key_field: {
			common:      false
			description: "The name of the log field to use to determine if the event should be passed. This defaults to the [global `message_key` option][docs.reference.global-options#message_key]."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["message"]
			}
		}
		pass_list: {
			common:      true
			description: "A list of regular expression patterns to exclude events from sampling. If an event's key field (see `key_field`) matches _any_ of these patterns it will _not_ be sampled."
			required:    false
			warnings: []
			type: array: {
				default: null
				items: type: string: examples: ["[error]", "field2"]
			}
		}
		rate: {
			description: "The rate at which events will be forwarded, expressed as 1/N. For example, `rate = 10` means 1 out of every 10 events will be forwarded and the rest will be dropped."
			required:    true
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
		vector_events_discarded_total: _vector_events_discarded_total
	}
}
