package metadata

remap: errors: "660": {
	title: "Non-Boolean negation"

	description: """
		You've used the negation operator to negate a non-Boolean expression.
		"""

	rationale: """
		Only non-Boolean values can be used with the negation operator (`!`). The expression `!false`, for example,
		produces `true`, whereas `!"hello"` is a meaningless non-expression.
		"""

	resolution: """
		Use the negation operator only with Boolean expressions.
		"""

	examples: [
		{
			"title": "\(title)"
			source: #"""
				!47
				"""#
			raises: compiletime: #"""
				error: \#(title)
				┌─ :1:1
				│
				1 │ !7
				│ ^- this expression resolves to "integer"
				│ │
				│ negation only works on boolean values
				│
				"""#
			diff: #"""
				- 	!47
				+# 	!(47 == 48)
				"""#
		},
	]
}
