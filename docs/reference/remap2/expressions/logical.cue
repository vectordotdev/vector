package metadata

remap2: expressions: logical: {
	title: "Assignment"
	description: """
		A "logical" expression compares two boolean expressions and produces a boolean.
		"""
	return: """
		Returns the same boolean type as the expressions (operands).
		"""

	grammar: {
		source: """
			boolean_expression ~ operator ~ boolean_expression
			"""
		definitions: {
			boolean_expression: {
				description:	"""
					The `expression` (operand) can be any expression that returns a valid type as defined by the
					`operator`.
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
				true && "foo"
				"""#
			return: "foo"
		},
		{
			title: "OR (boolean)"
			source: #"""
				false || "foo"
				"""#
			return: "foo"
		},
		{
			title: "OR (null)"
			source: #"""
				null || "foo"
				"""#
			return: "foo"
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
