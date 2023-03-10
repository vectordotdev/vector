package metadata

remap: functions: random_float: {
	category: "Random"
	description: """
		Returns random float between [min, max).
		"""

	arguments: [
		{
			name:        "min"
			description: "Minimum value (inclusive)"
			required:    true
			type: ["float"]
		},
		{
			name:        "max"
			description: "Maximum value (exclusive)"
			required:    true
			type: ["float"]
		},
	]
	internal_failure_reasons: [
		"max is not greater than min",
	]
	return: types: ["float"]

	examples: [
		{
			title: "Random float from 0 to 10, not including 10"
			source: """
				random_float!(0, 10)
				"""
			return: 1.123
		},
	]
}
