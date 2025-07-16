package metadata

generated: components: transforms: incremental_to_absolute: configuration: expire_metrics_secs: {
	description: """
		The amount of time, in seconds, that incremental metrics will persist in the internal
		metrics cache after having not been updated before they expire and are removed.
		Once removed, incremental counters are reset to 0.
		"""
	required: false
	type: uint: {
		default: 120
		examples: [
			"240",
		]
		unit: "seconds"
	}
}
