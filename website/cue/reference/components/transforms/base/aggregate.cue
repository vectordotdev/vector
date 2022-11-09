package metadata

base: components: transforms: aggregate: configuration: interval_ms: {
	description: """
		The interval between flushes, in milliseconds.

		Over this period metrics with the same series data (name, namespace, tags, …) will be aggregated.
		"""
	required: false
	type: uint: default: 10000
}
