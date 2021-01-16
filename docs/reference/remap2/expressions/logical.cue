package metadata

remap2: expressions: logical: {
	title: "Assignment"
	description: """
		An "logical" expression compares two boolean expressions and produces a boolean.
		"""
	return: """
		Returns the same boolean type as the expressions operands.
		"""

	grammar: {
		source: """
			expression ~ operator ~ expression
			"""
		definitions: {
			expression: {
				description:	"""
					The `expression` can be any expression that returns a valid type as defined by the `operator`.
					"""
			}
			operator: {
				description:	"""
					The `operator` defines the operation performed on the left-hand and right-hand side operations.
					"""
				enum: {
					"&&": "Conditional AND."
					"||": "Conditional OR."
					"!": "NOT."
				}
			}
		}
	}

	examples: [
		{
			title: "AND"
			source: #"""
				true && true
				"""#
			return: true
		},
		{
			title: "OR"
			source: #"""
				false || true
				"""#
			return: true
		},
		{
			title: "NOT"
			source: #"""
				!false
				"""#
			return: true
		},
	]
}
