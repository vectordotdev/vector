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
			title: "Push an item onto an array"
			input: log: array: [72.5, false, [1, 2, 3]]
			source: """
				.array = push(.array, "booper")
				"""
			output: log: array: [72.5, false, [1, 2, 3], "booper"]
		},
	]
}
