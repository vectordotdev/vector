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
			description: "The name of the log field to use to determine if the event should be passed. An event without this field will always be index rated."
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
		property: {
			description: "The property of event being rated."
			required:    false
			common:      true
			warnings: []
			type: string: {
				default: "index"
				enum: {
					"index": """
						Index of event determined by enumeration in the transform.
						Has a consistent, configured rate of sampling.
						"""
					"hash": """
						Hash of key field defined by `key_field` option. 
						Consistently samples the same events.
						Values in the field should be uniformly distributed, otherwise 
						actual rate of sampling may differ from the configured one.  
						"""
				}
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
