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
		Appends the specified item to an array and returns the new array. The item can be of any VRL
		type and is appended even if an item with the same value is already present in the array.
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
