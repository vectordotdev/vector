package metadata

remap: functions: flatten: {
	category: "Enumerate"
	description: #"""
		Flattens the `value` into a single-level representation.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The array or object to flatten."
			required:    true
			type: ["array", "object"]
		},
		{
			name:        "separator"
			description: "The separator to be used to join nested keys"
			required:    false
			default:     "."
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["array", "object"]
		rules: [
			"The return type matches the `value` type.",
		]
	}

	examples: [
		{
			title: "Flatten array"
			source: #"""
				flatten([1, [2, 3, 4], [5, [6, 7], 8], 9])
				"""#
			return: [1, 2, 3, 4, 5, 6, 7, 8, 9]
		},
		{
			title: "Flatten object"
			source: #"""
				flatten({
					"parent1": {
						"child1": 1,
						"child2": 2
					},
					"parent2": {
						"child3": 3
					}
				})
				"""#
			return: {
				"parent1.child1": 1
				"parent1.child2": 2
				"parent2.child3": 3
			}
		},
	]
}
