package metadata

remap: functions: append: {
	category: "Array"
	description: """
		Appends the `items` to the end of the `value`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The array"
			required:    true
			type: ["array"]
		},
		{
			name:        "items"
			description: "The items to append"
			required:    true
			type: ["array"]
		},
	]
	internal_failure_reasons: []
	return: types: ["array"]

	examples: [
		{
			title: "Append to an array"
			source: """
				 append([1, 2], [3, 4])
				"""
			return: [1, 2, 3, 4]
		},
	]
}
