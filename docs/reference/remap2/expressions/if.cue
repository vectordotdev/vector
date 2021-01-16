package metadata

remap2: expressions: if: {
	title: "If"
	description: """
		"If" expressions specify the conditional execution of two branches according to the value of a boolean
		expression. If the expression evaluates to `true`, the "if" branch is executed, otherwise, if present, the
		"else" branch is executed.
		"""
	return: """
		The return of the "if" expression is the result of the last expression evaluated.
		"""

	grammar: {
		source: """
			"if" ~ if_condition ~ block ~ ("else if" ~ if_condition ~ block)* ~ ("else" ~ block)?
			"""
		definitions: {
			if_condition: {
				description: """
					The `if_condition` must be a boolean expression. If a boolean is not returned a compile-time
					error will be raised.
					"""
			}
		}
	}

	examples: [
		{
			title: "True if expression"
			source: #"""
				if (true) {
					"Hello, World!"
				}
				"""#
			return: "Hello, World!"
		},
		{
			title: "False if expression"
			source: #"""
				if (false) {
					# not evaluated
				}
				"""#
			return: null
		},
		{
			title: "If/else expression"
			source: #"""
				if (false) {
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
				if (false) {
					# not evaluated
				} else if (false) {
					# not evaluated
				} else {
					"Hello, World!"
				}
				"""#
			return: "Hello, World!"
		},
	]
}
