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
	category: "object"
	description: #"""
		Returns a nested array or map that has been flattened to a single level.
		"""#
	examples: [
		{
			title: "Array Flatten"
			input: {
				array: [1, [2, 3, 4], [5, [6, 7], 8], 9]
			}
			source: #"""
				.flattened = flatten(.array)
				"""#
			output: {
				array: [1, [2, 3, 4], [5, [6, 7], 8], 9]
				flattened: [1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
		},
		{
			title: "Map Flatten"
			input: {
				object: {"grandparent": {"parent1": {"child1": 1
										"child2": 2
				}
					"parent2": {"child1": 3}
				}
				}
			}
			source: #"""
				.flattened = flatten(.object)
				"""#
			output: {
				object: {"grandparent": {"parent1": {"child1": 1
										"child2": 2
				}
					"parent2": {"child1": 3}
				}
				}
				flattened: {"grandparent.parent1.child1": 1
										"grandparent.parent1.child2": 2
										"grandparent.parent2.child1": 2
				}

			}
		},
		{
			title: "Error"
			input: {
				text: "this cannot be flattened"
			}
			source: #"""
				.flattened = flatten(.text)
				"""#
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
