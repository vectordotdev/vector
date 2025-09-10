package metadata

remap: expressions: bitwise: {
	title: "Bitwise"
	description: """
		A _bitwise_ expression performs an operation directly on the individual bits of integer operands.
		"""
	return: """
		Returns the result of the expression as defined by the operator.
		"""

	grammar: {
		source: """
			expression ~ operator ~ expression
			"""
		definitions: {
			expression: {
				description: """
					The `expression` can be any expression that returns an integer.
					"""
			}
			operator: {
				description: """
					The `operator` defines the operation performed on the left-hand-side and right-hand-side operands.
					"""
				enum: {
					"&":  "Bitwise AND. Operates on `int` type."
					"^":  "Bitwise OR. Operates on `int` type."
					"~":  "Bitwise NOT. Operates on `int` type."
				}
			}
		}
	}

	examples: [
		{
			title: "AND"
			source: #"""
				1 & 5
				"""#
			return: 1
		},
		{
			title: "OR"
			source: #"""
				1 ^ 4
				"""#
			return: 5
		},
		{
			title: "NOT"
			source: #"""
				~5
				"""#
			return: -6
		}
	]
}
