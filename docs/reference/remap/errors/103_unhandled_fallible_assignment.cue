package metadata

remap: errors: "103": {
	title:       "Unhandled fallible assignment"
	description: """
		The right-hand side of an [assignment expression](\(urls.vrl_expressions)\(remap.literals.regular_expression.anchor))
		can fail and the error is not being [handled](\(urls.vrl_error_handling)).
		"""
	rationale:   """
		VRL is [fail-safe](\(urls.vrl_error_safety)) and requires that all possible runtime errors be handled. This
		contributes heavily to VRL's [safety principle](\(urls.vrl_safety)).
		"""
	resolution:  """
		Handle the error using one of the [documented error handling strategies](\(urls.vrl_error_handling)).
		"""

	examples: [...{
		input: log: message: "key=value"
		source: #"""
			. |= parse_key_value(.message)
			"""#
		raises: compiletime: #"""
			error: \#(title)
			  ┌─ :1:1
			  │
			1 │ . |= parse_key_value(.message)
			  │ ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
			  │ │
			  │ This assingment does not handle errors
			  │
			"""#
	}]

	examples: [
		{
			"title": "\(title) (coalesce)"
			diff: #"""
				-. |= parse_key_value(.message)
				+. |= parse_key_value(.message) ?? {}
				"""#
		},
		{
			"title": "\(title) (raise & abort)"
			diff: #"""
				-. |= parse_key_value(.message)
				+. |= parse_key_value!(.message)
				"""#
		},
		{
			"title": "\(title) (assignment)"
			diff: #"""
				-. |= parse_key_value(.message)
				+., err |= parse_key_value(.message)
				"""#
		},
	]
}
