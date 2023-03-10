package metadata

remap: functions: random_bool: {
	category: "Random"
	description: """
		Returns a random boolean.
		"""

	arguments: []
	internal_failure_reasons: []
	return: types: ["boolean"]

	examples: [
		{
			title: "Random boolean"
			source: """
				random_bool()
				"""
			return: true
		},
	]
}
