package metadata

remap: errors: "103": {
	title:       "Unhandled assignment runtime error"
	description: """
		The right-hand side of an [assignment expression](\(urls.vrl_expressions)#\(remap.literals.regular_expression.anchor))
		is fallible and can produce a [runtime error](\(urls.vrl_runtime_errors)), but the error is not being
		[handled](\(urls.vrl_error_handling)).
		"""
	rationale:   """
		VRL is [fail-safe](\(urls.vrl_fail_safety)) and requires that all possible runtime errors be handled. This
		contributes heavily to VRL's [safety principle](\(urls.vrl_safety)), ensuring that VRL programs are reliable
		once deployed.
		"""
	resolution:  """
		[Handle](\(urls.vrl_error_handling)) the runtime error by [assigning](\(urls.vrl_error_handling_assigning)),
		[coalescing](\(urls.vrl_error_handling_coalescing)), or [raising](\(urls.vrl_error_handling_raising)) the
		error.
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
			"title": "\(title) (coalescing)"
			diff: #"""
				-. |= parse_key_value(.message)
				+. |= parse_key_value(.message) ?? {}
				"""#
		},
		{
			"title": "\(title) (raising)"
			diff: #"""
				-. |= parse_key_value(.message)
				+. |= parse_key_value!(.message)
				"""#
		},
		{
			"title": "\(title) (assigning)"
			diff: #"""
				-. |= parse_key_value(.message)
				+., err |= parse_key_value(.message)
				"""#
		},
	]
}
