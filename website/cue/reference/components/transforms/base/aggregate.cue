package metadata

base: components: transforms: aggregate: configuration: interval_ms: {
	description: """
		The interval between flushes, in milliseconds.

		During this time frame, metrics with the same series data (name, namespace, tags, and so on) are aggregated.
		"""
	required: false
	type: uint: default: 10000
}
