package metadata

remap: functions: merge: {
	arguments: [
		{
			name:        "to"
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
	internal_failure_reasons: []
	return: ["string"]
	category: "Map"
	description: #"""
		Merges the `from` map provided into the `to` map.

		* If a key exists in both maps, the field from the `from` map is chosen.
		* If `deep` is specified, and a key exists in both maps, and both these fields are also maps, then those maps
		  will merge recursively as well.
		"""#
	examples: [
		{
			title: "Shallow object merge"
			input: log: {
				map1: {
					parent1: {
						child1: 1
						child2: 2
					}
					parent2: child3: 3
				}
				map2: parent1: {
					child2: 4
					child5: 5
				}
			}
			source: #"""
				merge(.map1, .map2, deep: false)
				"""#
			output: log: {
				map1: {
					parent1: {
						child2: 4
						child5: 5
					}
					parent2: child3: 3
				}
				map2: parent1: {
					child2: 4
					child5: 5
				}
			}
		},
		{
			title: "Deep"
			input: log: {
				map1: {
					parent1: {
						child1: 1
						child2: 2
					}
					parent2: child3: 3
				}
				map2: parent1: {
					child2: 4
					child5: 5
				}
			}
			source: #"""
				merge(.map1, .map2, deep: true)
				"""#
			output: log: {
				map1: {
					parent1: {
						child1: 1
						child2: 4
						child5: 5
					}
					parent2: child3: 3
				}
				map2: parent1: {
					child2: 4
					child5: 5
				}
			}
		},
	]
}
