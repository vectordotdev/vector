package metadata

remap: functions: append: {
	category: "Array"
	description: """
		Appends each item in the `items` array to the end of the `value` array.
		"""

	arguments: [
		{
			name:        "value"
			description: "The initial array."
			required:    true
			type: ["array"]
		},
		{
			name:        "items"
			description: "The items to append."
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
