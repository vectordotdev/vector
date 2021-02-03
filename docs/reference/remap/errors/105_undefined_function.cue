package metadata

remap: errors: "105": {
	title:       "Undefined function"
	description: """
		A [function call expression](\(urls.vrl_expressions)#\(remap.literals.regular_expression.anchor)) invokes an
		unknown function.
		"""
	rationale:   null
	resolution: """
		This is typically due to a typo, correcting the function name will resolve this.
		"""

	examples: [
		{
			"title": "\(title) (typo)"
			source: #"""
				parse_keyvalue(.message)
				"""#
			raises: compiletime: #"""
				error: \#(title)
				  ┌─ :1:1
				  │
				1 │ parse_keyvalue(.message)
				  │ ^^^^^^^^^^^^^^
				  │ │
				  │ Undefined function
				  │
				"""#
			diff: #"""
				-parse_keyvalue(.message)
				+parse_key_value(.message)
				"""#
		},
	]
}
