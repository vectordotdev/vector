package metadata

remap: errors: "109": {
	title:       "Cannot abort function"
	description: """
		A [function call expression](\(urls.vrl_expressions)#\(remap.literals.regular_expression.anchor)) cannot end
		with `!` unless it is _fallible_. If a function cannot produce a runtime error, then it will not have an abort
		variant that ends with `!`.
		"""
	rationale:   null
	resolution: """
		Remove the `!` from the end of the function name.
		"""

	examples: [
		{
			"title": title
			source: #"""
				downcase!(.message)
				"""#
			raises: compiletime: #"""
				error: \#(title)
				  ┌─ :1:1
				  │
				1 │ downcase!(.message)
				  │         ^
				  │         │
				  │         This function is not fallible
				  │
				"""#
			diff: #"""
				-downcase!(.message)
				+downcase(.message)
				"""#
		},
	]
}
