package metadata

generated: components: transforms: sample: configuration: {
	exclude: {
		description: "A logical condition used to exclude events from sampling."
		required:    false
		type: condition: {}
	}
	group_by: {
		description: """
			The value to group events into separate buckets to be sampled independently.

			If left unspecified, or if the event doesn't have `group_by`, then the event is not
			sampled separately.
			"""
		required: false
		type: string: {
			examples: ["{{ service }}", "{{ hostname }}-{{ service }}"]
			syntax: "template"
		}
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

			This option cannot be combined with `ratio_field` or `rate_field`.
			"""
		required: false
		type: string: examples: ["message"]
	}
	rate: {
		description: """
			The rate at which events are forwarded, expressed as `1/N`.

			For example, `rate = 1500` means 1 out of every 1500 events are forwarded and the rest are
			dropped. This differs from `ratio` which allows more precise control over the number of events
			retained and values greater than 1/2. It is an error to provide a value for both `rate` and `ratio`.
			"""
		required: false
		type: uint: examples: [
			1500,
		]
	}
	rate_field: {
		description: """
			The event field whose integer value is used as the sampling rate on a per-event basis, expressed as `1/N`.

			The value must be a positive integer to be considered valid. If the field is missing or invalid,
			static sampling settings (`rate` or `ratio`) are used as a fallback.
			This option cannot be used together with `ratio_field`.
			"""
		required: false
		type: string: examples: ["sample_rate_n"]
	}
	ratio: {
		description: """
			The rate at which events are forwarded, expressed as a percentage

			For example, `ratio = .13` means that 13% out of all events on the stream are forwarded and
			the rest are dropped. This differs from `rate` allowing the configuration of a higher
			precision value and also the ability to retain values of greater than 50% of all events. It is
			an error to provide a value for both `rate` and `ratio`.
			"""
		required: false
		type: float: examples: [
			0.13,
		]
	}
	ratio_field: {
		description: """
			The event field whose numeric value is used as the sampling ratio on a per-event basis.

			The value must be in `(0, 1]` to be considered valid. If the field is missing or invalid,
			static sampling settings (`rate` or `ratio`) are used as a fallback.
			This option cannot be used together with `rate_field`.
			"""
		required: false
		type: string: examples: ["sample_rate"]
	}
	sample_rate_key: {
		description: "The event key in which the sample rate is stored. If set to an empty string, the sample rate will not be added to the event."
		required:    false
		type: string: {
			default: "sample_rate"
			examples: ["sample_rate"]
		}
	}
}
