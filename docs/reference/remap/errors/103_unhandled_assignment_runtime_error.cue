package metadata

remap: errors: "103": {
	title:       "Unhandled assignment runtime error"
	description: """
		The right-hand side of an [assignment expression](\(urls.vrl_expressions)#\(remap.literals.regular_expression.anchor))
		is fallible and can produce a [runtime error](\(urls.vrl_runtime_errors)), but the error isn't being
		[handled](\(urls.vrl_error_handling)).
		"""
	rationale:   remap._fail_safe_blurb
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
