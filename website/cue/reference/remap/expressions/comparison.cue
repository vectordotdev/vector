package metadata

remap: expressions: comparison: {
	title:       "Comparison"
	description: """
		A _comparison_ expression compares two expressions (operands) and produces a Boolean as defined by the
		operator. Please refer to the [match function](\(urls.vrl_match_function)) for matching a string against a regex.
		"""
	return: """
		Returns a Boolean as defined by the operator.
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
					The `operator` defines the operation performed on the left-hand and right-hand side operations.
					"""
				enum: {
					"==": "Equal. Operates on all types."
					"!=": "Not equal. Operates on all types."
					">=": "Greater than or equal. Operates on `int` and `float` types."
					">":  "Greater than. Operates on `int` and `float` types."
					"<=": "Less than or equal. Operates on `int` and `float` types."
					"<":  "Less than. Operates on `int` and `float` types."
				}
			}
		}
	}

	examples: [
		{
			title: "Equal integers"
			source: #"""
				1 == 1
				"""#
			return: true
		},
		{
			title: "Equal integer and float"
			source: #"""
				1 == 1.0
				"""#
			return: true
		},
		{
			title: "Not equal"
			source: #"""
				1 != 2
				"""#
			return: true
		},
		{
			title: "Equal string"
			source: #"""
				x = "foo"
				x == "foo"
				"""#
			return: true
		},
		{
			title: "Not equal strings"
			source: #"""
				"foo" != "bar"
				"""#
			return: true
		},
		{
			title: "Greater than or equal"
			source: #"""
				2 >= 2
				"""#
			return: true
		},
		{
			title: "Greater than"
			source: #"""
				2 > 1
				"""#
			return: true
		},
		{
			title: "Less than or equal"
			source: #"""
				2 <= 2
				"""#
			return: true
		},
		{
			title: "Less than"
			source: #"""
				1 < 2
				"""#
			return: true
		},
	]
}
