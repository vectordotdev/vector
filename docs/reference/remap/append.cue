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
			name:        "item"
			description: "The item to append"
			required:    true
			type: ["any"]
		},
	]
	return: ["array"]
	category: "Array"
	description: """
		Appends the specified item to an array. The item is appended regardless
		of what is currently in the array.
		"""
	examples: [
		{
			title: "Mixed array"
			input: {
				kitchen_sink: [72.5, false, [1, 2, 3]]
				item: "booper"
			}
			source: """
				.kitchen_sink = append(.kitchen_sink, .item)
				"""
			output: {
				kitchen_sink: [72.5, false, [1, 2, 3], "booper"]
			}
		},
	]
}
