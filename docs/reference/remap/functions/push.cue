package metadata

remap: functions: push: {
	category: "Array"
	description: """
		Adds the `item` to the end of the `value` array.
		"""

	arguments: [
		{
			name:        "value"
			description: "The array"
			required:    true
			type: ["array"]
		},
		{
			name:        "item"
			description: "The item to push"
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["array"]
		rules: [
			"A new array is returned, the `value` is not modified in place.",
		]
	}

	examples: [
		{
			title: "Push an item onto an array"
			source: """
				push([1, 2], 3)
				"""
			return: [1, 2, 3]
		},
	]
}
