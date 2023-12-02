package metadata

remap: functions: random_int: {
	category: "Random"
	description: """
		Returns a random integer between [min, max).
		"""

	arguments: [
		{
			name:        "min"
			description: "Minimum value (inclusive)."
			required:    true
			type: ["integer"]
		},
		{
			name:        "max"
			description: "Maximum value (exclusive)."
			required:    true
			type: ["integer"]
		},
	]
	internal_failure_reasons: [
		"`max` is not greater than `min`.",
	]
	return: types: ["integer"]

	examples: [
		{
			title: "Random integer from 0 to 10, not including 10"
			source: """
				i = random_int(0, 10)
				i >= 0 && i < 10
				"""
			return: true
		},
	]
}
