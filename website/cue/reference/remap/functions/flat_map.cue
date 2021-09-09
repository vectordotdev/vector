package metadata

remap: functions: flat_map: {
	category: "Enumerate"
	description: #"""
		Map an array `value` to an inner array field and into a single-level representation.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The array to flat_map."
			required:    true
			type: ["array"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["array"]
		rules: [
			"The return type matches the `value` type.",
		]
	}

	examples: [
		{
			title: "Flat Map array"
			source: #"""
				flat_map([{"a": [1]}, {"a": [2]}, {"a": [3]}], .a)
				"""#
			return: [1, 2, 3]
		},
	]
}
