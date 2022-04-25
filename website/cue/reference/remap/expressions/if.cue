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
					The predicate can contain multiple expressions. Multiple expression predicates must be wrapped in
					parentheses. The expressions need to be separated by either a semicolon (`;`) or a new line.
					"""
			}
		}
	}

	examples: [
		{
			title: "True if expression"
			input: log: foo: true
			source: #"""
				if .foo == true {
					"Hello, World!"
				}
				"""#
			return: "Hello, World!"
		},
		{
			title: "False if expression"
			input: log: foo: false
			source: #"""
				if .foo == true {
					# not evaluated
					"Hello, World!"
				}
				"""#
			return: null
		},
		{
			title: "If/else expression"
			input: log: foo: false
			source: #"""
				if .foo == true {
					# not evaluated
					null
				} else {
					"Hello, World!"
				}
				"""#
			return: "Hello, World!"
		},
		{
			title: "If/else if/else expression"
			input: log: foo: true
			source: #"""
				if .foo == false {
					# not evaluated
					null
				} else if .foo == false {
					# not evaluated
					null
				} else {
					"Hello, World!"
				}
				"""#
			return: "Hello, World!"
		},
		{
			title: "Multiline expression"
			source: #"""
				x = 3
				if (x = x + 1; x == 5) {
					# not evaluated
					null
				} else if (
					x = x + 1
					x == 5
				) {
					"Hello, World!"
				}
				"""#
			return: "Hello, World!"
		},

	]
}
