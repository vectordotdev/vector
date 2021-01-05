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
	return: ["array"]
	category: "Array"
	description: """
		Adds each item from an array to the end of another array. The expression
		`append([1, 2, 3], [4, 5, 6])`, for example, would produce the array `[1, 2, 3, 4, 5, 6]`.
		The items in both arrays can be of any VRL type.
		"""
	examples: [
		{
			title: "Mixed array"
			input: {
				kitchen_sink: [72.5, false, [1, 2, 3]]
				items: ["booper", "bopper"]
			}
			source: """
				.kitchen_sink = append(.kitchen_sink, .items)
				"""
			output: {
				kitchen_sink: [72.5, false, [1, 2, 3], "booper", "bopper"]
			}
		},
	]
}
