package metadata

remap: expressions: arithmetic: {
	title: "Arithmetic"
	description: """
		An _arithmetic_ expression performs an operation on two expressions (operands) as defined by the operator.

		Although arithmetic is commonly applied to numbers, you can use it with other types as well, such as strings.
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
					The `expression` can be any expression that returns a valid type as defined by the `operator`.
					"""
			}
			operator: {
				description: """
					The `operator` defines the operation performed on the left-hand- and right-hand-side operands.
					"""
				enum: {
					"+":  "Sum. Operates on `int`, `float`, and `string` types."
					"-":  "Difference. Operates on `int` and `float` types."
					"*":  "Multiplication. Operates on `int` and `float` types."
					"/":  "Float division. Operates on `int` and `float` types. _Always_ produces a `float`."
					"//": "Integer division. Operates on `int` and `float` types. _Always_ produces a `int`."
					"%":  "Remainder. Operates on `int` and `float` types. _Always_ produces an `int`."
				}
			}
		}
	}

	examples: [
		{
			title: "Sum (int)"
			source: #"""
				1 + 1
				"""#
			return: 2
		},
		{
			title: "Sum (float)"
			source: #"""
				1.0 + 1.0
				"""#
			return: 2.0
		},
		{
			title: "Sum (numeric)"
			source: #"""
				1 + 1.0
				"""#
			return: 2.0
		},
		{
			title: "Sum (string)"
			source: #"""
				"Hello" + ", " + "World!"
				"""#
			return: "Hello, World!"
		},
		{
			title: "Difference (int)"
			source: #"""
				2 - 1
				"""#
			return: 1
		},
		{
			title: "Difference (float)"
			source: #"""
				2.0 - 1.0
				"""#
			return: 1.0
		},
		{
			title: "Difference (numeric)"
			source: #"""
				2.0 - 1
				"""#
			return: 1.0
		},
		{
			title: "Multiplication (int)"
			source: #"""
				2 * 1
				"""#
			return: 2
		},
		{
			title: "Multiplication (float)"
			source: #"""
				2.0 * 1.0
				"""#
			return: 2.0
		},
		{
			title: "Multiplication (numeric)"
			source: #"""
				2.0 * 1
				"""#
			return: 2.0
		},
		{
			title: "Float division (int)"
			source: #"""
				2 / 1
				"""#
			return: 2.0
		},
		{
			title: "Float division (float)"
			source: #"""
				2.0 / 1.0
				"""#
			return: 2.0
		},
		{
			title: "Float division (numeric)"
			source: #"""
				2.0 / 1
				"""#
			return: 2.0
		},
		{
			title: "Remainder"
			source: #"""
				mod(3, 2)
				"""#
			return: 1
		},
	]
}
