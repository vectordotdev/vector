package metadata

remap: functions: pop: {
	category: "Array"
	description: """
		Removes the last item from the `value` array.
		"""

	arguments: [
		{
			name:        "value"
			description: "The target array."
			required:    true
			type: ["array"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["array"]
		rules: [
			"The original `value` is not modified.",
		]
	}

	examples: [
		{
			title: "Pop an item from an array"
			source: """
				pop([1, 2, 3])
				"""
			return: [1, 2]
		},
	]
}
