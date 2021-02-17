package metadata

remap: expressions: if: {
	title: "If"
	description: """
		An _if_ expression specifies the conditional execution of two branches according to the value of a Boolean
		expression. If the Boolean expression evaluates to `true`, the "if" branch is executed, otherwise the "else"
		branch is executed (if present).
		"""
	return: """
		The result of the last expression evaluated in the executed branch or null if no expression is evaluated.
		"""

	grammar: {
		source: """
			"if" ~ predicate ~ block ~ ("else if" ~ predicate ~ block)* ~ ("else" ~ block)?
			"""
		definitions: {
			predicate: {
				description: """
					The `predicate` _must_ be an expression that resolves to a Boolean. If a Boolean isn't returned, a
					compile-time error is raised.
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
