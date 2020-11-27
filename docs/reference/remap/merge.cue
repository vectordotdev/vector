package metadata

remap: functions: merge: {
	arguments: [
		{
			name:        "to_path"
			description: "The path of the object to merge into."
			required:    true
			type: ["string"]
		},
		{
			name:        "from"
			description: "The object to merge from."
			required:    true
			type: ["map"]
		},
		{
			name:        "deep"
			description: "If true a deep merge is performed, otherwise only top level fields are merged."
			required:    false
			default:     false
			type: ["boolean"]
		},
	]
	return: ["string"]
	category: "text"
	description: #"""
		Merges the `from` map provided into the `to_path` path specified, which must specify an existing map.
		If a key exists in both maps, the field from the `from` map is chosen.
		If `deep` is specified, if a key exists in both maps, and both these fields are also maps merge will recursively
		merge these fields.
		"""#
	examples: [
		{
			title: "Shallow"
			input: {
				map1: {"parent1": {"child1": 1
								"child2": 2
				}
					"parent2": {"child3": 3}
				}
				map2: {"parent1": {"child2": 4
								"child5": 5
				}
				}
			}
			source: #"""
				merge(".map1", .map2, deep = false)
				"""#
			output: {
				map1: {"parent1": {"child2": 4
								"child5": 5
				}
					"parent2": {"child3": 3}
				}
			}
		},
		{
			title: "Deep"
			input: {
				map1: {"parent1": {"child1": 1
								"child2": 2
				}
					"parent2": {"child3": 3}
				}
				map2: {"parent1": {"child2": 4
								"child5": 5
				}
				}
			}
			source: #"""
				merge(".map1", .map2, deep = true)
				"""#
			output: {
				map1: {"parent1": {"child1": 1
								"child2": 4
								"child5": 5
				}
					"parent2": {"child3": 3}
				}
				map2: {"parent1": {"child2": 4
								"child5": 5
				}
				}
			}
		},
		{
			title: "Error"
			input: {
				map1: "just a string"
				map2: {"parent1": {"child2": 4
								"child5": 5
				}
				}
			}
			source: #"""
				merge(".map1", .map2, deep = true)
				"""#
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
