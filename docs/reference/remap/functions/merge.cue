package metadata

remap: functions: merge: {
	category: "Object"
	description: """
		Merges the `from` object into the `to` object.
		"""

	arguments: [
		{
			name:        "to"
			description: "The object to merge into."
			required:    true
			type: ["string"]
		},
		{
			name:        "from"
			description: "The object to merge from."
			required:    true
			type: ["object"]
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
	return: {
		types: ["object"]
		rules: [
			#"If a key exists in both objects, the field from the `from` object is chosen."#,
			#"If `deep` is specified, and a key exists in both objects, and both these fields are also objects, then those objects will merge recursively as well."#,
		]
	}

	examples: [
		{
			title: "Object merge (shallow)"
			source: #"""
				merge(
					{
						"parent1": {
							"child1": 1,
							"child2": 2
						},
						"parent2": {
							"child3": 3
						}
					},
					{
						"parent1": {
							"child2": 4,
							"child5": 4
						}
					}
				)
				"""#
			return: {
				parent1: {
					child2: 4
					child5: 5
				}
				parent2: child3: 3
			}
		},
		{
			title: "Object merge (deep)"
			source: #"""
				merge(
					{
						"parent1": {
							"child1": 1,
							"child2": 2
						},
						"parent2": {
							"child3": 3
						}
					},
					{
						"parent1": {
							"child2": 4,
							"child5": 4
						}
					},
					deep: true
				)
				"""#
			return: {
				parent1: {
					child1: 1
					child2: 4
					child5: 5
				}
				parent2: child3: 3
			}
		},
	]
}
