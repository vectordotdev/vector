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
			type: ["object"]
		},
		{
			name:        "from"
			description: "The object to merge from."
			required:    true
			type: ["object"]
		},
		{
			name:        "deep"
			description: "A deep merge is performed if `true`, otherwise only top-level fields are merged."
			required:    false
			default:     false
			type: ["boolean"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["object"]
		rules: [
			#"The field from the `from` object is chosen if a key exists in both objects."#,
			#"""
				Objects are merged recursively if `deep` is specified, a key exists in both objects, and both of those
				fields are also objects.
				"""#,
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
							"child5": 5
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
							"child5": 5
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
