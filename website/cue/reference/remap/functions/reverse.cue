package metadata

remap: functions: reverse: {
	category: "Enumerate"
	description: #"""
		Reverse the `value` into a single-level representation.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The array to reverse."
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
			title: "Reverse array"
			source: #"""
				reverse([1, 2, 3])
				"""#
			return: [3, 2, 1]
		},
	]
}
