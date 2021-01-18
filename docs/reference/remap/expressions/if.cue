package metadata

remap: expressions: if: {
	title: "If"
	description: """
		An _if_ expression specifies the conditional execution of two branches according to the value of a boolean
		expression. If the boolean expression evaluates to `true`, the "if" branch is executed, otherwise, if present,
		the "else" branch is executed.
		"""
	return: """
		The result of the last expression evaluated in the executed branch or null if no expression is evaluated.
		"""

	grammar: {
		source: """
			"if" ~ boolean_expression ~ block ~ ("else if" ~ boolean_expression ~ block)* ~ ("else" ~ block)?
			"""
		definitions: {
			boolean_expression: {
				description: """
					The `boolean_expression` must be an expression that resolves to a boolean. If a boolean is not
					returned a compile-time error will be raised.
					"""
			}
		}
	}

	examples: [
		{
			title: "True if expression"
			source: #"""
				if true {
					"Hello, World!"
				}
				"""#
			return: "Hello, World!"
		},
		{
			title: "False if expression"
			source: #"""
				if false {
					# not evaluated
				}
				"""#
			return: null
		},
		{
			title: "If/else expression"
			source: #"""
				if false {
					# not evaluated
				} else {
					"Hello, World!"
				}
				"""#
			return: "Hello, World!"
		},
		{
			title: "If/else if/else expression"
			source: #"""
				if false {
					# not evaluated
				} else if false {
					# not evaluated
				} else {
					"Hello, World!"
				}
				"""#
			return: "Hello, World!"
		},
	]
}
