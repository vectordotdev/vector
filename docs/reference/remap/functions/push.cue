package metadata

remap: functions: push: {
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
	return: ["array"]
	category: "Array"
	description: """
		Adds the provided `item` to the end of the `value` array.

		The `push` function does _not_ change the array in place.
		"""
	examples: [
		{
			title: "Push an item onto an array (new array)"
			input: log: kitchen_sink: [72.5, false, [1, 2, 3]]
			source: """
				.kitchen_sink = push(.kitchen_sink, "booper")
				"""
			output: log: kitchen_sink: [72.5, false, [1, 2, 3], "booper"]
		},
		{
			title: "Push an item onto an array (same array)"
			input: log: kitchen_sink: [72.5, false, [1, 2, 3]]
			source: """
				.kitchen_sink = push(.kitchen_sink, "booper")
				"""
			output: log: kitchen_sink: [72.5, false, [1, 2, 3], "booper"]
		},
	]
}
