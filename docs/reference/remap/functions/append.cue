package metadata

remap: functions: append: {
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
	return: ["array"]
	category: "Array"
	description: """
		Adds each item from an array to the end of another array.
		"""
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
