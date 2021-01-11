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
	return: ["array", "map"]
	category: "Enumerate"
	description: #"""
		Returns a nested array or map that has been flattened to a single level.
		"""#
	examples: [
		{
			title: "Flatten array"
			input: log: array: [1, [2, 3, 4], [5, [6, 7], 8], 9]
			source: #"""
				.array = flatten(.array)
				"""#
			output: log: array: [1, 2, 3, 4, 5, 6, 7, 8, 9]
		},
		{
			title: "Flatten map"
			input: log: object: grandparent: {
				parent1: {
					child1: 1
					child2: 2
				}
				parent2: child1: 3
			}
			source: #"""
				.object = flatten(.object)
				"""#
			output: log: object: {
				"grandparent.parent1.child1": 1
				"grandparent.parent1.child2": 2
				"grandparent.parent2.child1": 2
			}
		},
	]
}
