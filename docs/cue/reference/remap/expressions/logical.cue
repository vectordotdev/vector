package metadata

remap: expressions: logical: {
	title: "Logical"
	description: """
		A _logical_ expression compares two expressions (operands), short-circuiting on the last expression evaluated
		as defined by the operator.
		"""
	return: """
		Returns the last expression (operand) evaluated as defined by the operator.
		"""

	grammar: {
		source: """
			expression ~ operator ~ expression
			"""
		definitions: {
			expression: {
				description: """
					The `expression` (operand) can be any expression that returns a valid type as defined by the
					`operator`.
					"""
			}
			operator: {
				description: """
					The `operator` defines the operation performed on the left-hand- and right-hand-side operations.
					"""
				enum: {
					"&&": "Conditional AND. Supports boolean expressions only."
					"||": "Conditional OR. Supports any expression."
					"!":  "NOT. Supports boolean expressions only."
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
