package metadata

base: components: transforms: sample: configuration: {
	exclude: {
		description: "A logical condition used to exclude events from sampling."
		required:    false
		type: condition: {}
	}
	key_field: {
		description: """
			The name of the field whose value is hashed to determine if the event should be
			sampled.

			Each unique value for the key creates a bucket of related events to be sampled together
			and the rate is applied to the buckets themselves to sample `1/N` buckets.  The overall rate
			of sampling may differ from the configured one if values in the field are not uniformly
			distributed. If left unspecified, or if the event doesn’t have `key_field`, then the
			event is sampled independently.

			This can be useful to, for example, ensure that all logs for a given transaction are
			sampled together, but that overall `1/N` transactions are sampled.
			"""
		required: false
		type: string: examples: ["message"]
	}
	rate: {
		description: """
			The rate at which events are forwarded, expressed as `1/N`.

			For example, `rate = 1500` means 1 out of every 1500 events are forwarded and the rest are
			dropped.
			"""
		required: true
		type: uint: examples: [
			1500,
		]
	}
  sample_rate_key: {
    description: """
    The name of the log field used to add the sample rate to each event. If set to "", the rate will not be set.
    """
    required: false
		type: string: {
			default: "sample_rate"
			examples: ["sample_rate", ""]
		}
  }
}
