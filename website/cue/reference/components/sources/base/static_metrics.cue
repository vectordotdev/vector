package metadata

base: components: sources: static_metrics: configuration: {
	interval_secs: {
		description: "The interval between metric emitting, in seconds."
		required:    false
		type: float: {
			default: 1.0
			unit:    "seconds"
		}
	}
	metrics: {
		description: "Tag configuration for the `internal_metrics` source."
		required:    false
		type: array: {
			default: []
			items: type: object: options: {
				name: {
					description: "Name of the static metric"
					required:    false
					type: string: default: ""
				}
				tags: {
					description: "Key-value pairs representing tags and their values to add to the metric."
					required:    false
					type: object: options: "*": {
						description: "An individual tag - value pair."
						required:    true
						type: string: {}
					}
				}
				value: {
					description: "\"Observed\" value of the static metric"
					required:    false
					type: float: default: 0.0
				}
			}
		}
	}
	namespace: {
		description: "Overrides the default namespace for the metrics emitted by the source."
		required:    false
		type: string: default: "vector"
	}
}
