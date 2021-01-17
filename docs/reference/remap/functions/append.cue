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
	internal_failure_reason: null
	return: ["array"]
	category: "Array"
	description: """
		Adds each item from an array to the end of another array.
		"""
	examples: [
		{
			title: "Mixed array"
			input: log: {
				kitchen_sink: [72.5, false, [1, 2, 3]]
			}
			source: """
				.kitchen_sink = append(.kitchen_sink, ["booper", "bopper"])
				"""
			output: log: {
				kitchen_sink: [72.5, false, [1, 2, 3], "booper", "bopper"]
			}
		},
	]
}
