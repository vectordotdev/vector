package metadata

remap: errors: "106": {
	title:       "Function argument arity mismatch"
	description: """
		A [function call expression](\(urls.vrl_expressions)#\(remap.literals.regular_expression.anchor)) invokes a
		function with too many arguments.
		"""
	rationale:   null
	resolution: """
		Remove the extra arguments to adhere to the function's documented signature.
		"""

	examples: [
		{
			"title": title
			source: #"""
				parse_json(.message, pretty: true)
				"""#
			diff: #"""
				-parse_json(.message, pretty: true)
				+parse_json(.message)
				"""#
		},
	]
}
