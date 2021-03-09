package metadata

/*
Leave this out until we decide on a better system for classifying function-specific error codes

remap: errors: "109": {
	title:       "Cannot abort function"
	description: """
		A [function call expression](\(urls.vrl_expressions)#\(remap.literals.regular_expression.anchor)) can't end with
		`!` unless the function is fallible, that is, if it can produce a runtime error. If the function is infallible,
		it doesn't have an abort variant that ends with `!`.
		"""
	resolution: """
		Remove the `!` from the end of the function name.
		"""

	examples: [
		{
			"title": title
			source: #"""
				downcase!(.message)
				"""#
			diff: #"""
				-downcase!(.message)
				+downcase(.message)
				"""#
		},
	]
}
*/
