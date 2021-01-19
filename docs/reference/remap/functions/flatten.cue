package metadata

remap: functions: flatten: {
	arguments: [
		{
			name:        "value"
			description: "The array or map to flatten."
			required:    true
			type: ["array", "map"]
		},
	]
	internal_failure_reasons: []
	return: ["array", "map"]
	category: "Enumerate"
	description: #"""
		Returns a nested `array` or `map` that has been flattened to a single level.
		"""#
	examples: [
		{
			title: "Flatten array"
			source: #"""
				flatten([1, [2, 3, 4], [5, [6, 7], 8], 9])
				"""#
			return: [1, 2, 3, 4, 5, 6, 7, 8, 9]
		},
		{
			title: "Flatten map"
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
				"grandparent.parent1.child1": 1
				"grandparent.parent1.child2": 2
				"grandparent.parent2.child1": 2
			}
		},
	]
}
