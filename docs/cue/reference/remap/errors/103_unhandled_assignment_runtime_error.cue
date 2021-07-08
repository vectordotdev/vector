package metadata

remap: errors: "103": {
	title:       "Unhandled fallible assignment"
	description: """
		The right-hand side of this [assignment](\(urls.vrl_expressions)#regular-expression)
		is fallible (that is, it can produce a [runtime error](\(urls.vrl_runtime_errors))), but the error isn't
		[handled](\(urls.vrl_error_handling)).
		"""
	rationale:   remap._fail_safe_blurb
	resolution:  """
		[Handle](\(urls.vrl_error_handling)) the runtime error by either
		[assigning](\(urls.vrl_error_handling_assigning)) it, [coalescing](\(urls.vrl_error_handling_coalescing)) it, or
		[raising](\(urls.vrl_error_handling_raising)) it.
		"""

	examples: [...{
		input: log: message: "key=value"
		source: #"""
			. = parse_key_value(.message)
			"""#
	}]

	examples: [
		{
			"title": "\(title) (coalescing)"
			diff: #"""
				-. = parse_key_value(.message)
				+. = parse_key_value(.message) ?? {}
				"""#
		},
		{
			"title": "\(title) (raising)"
			diff: #"""
				-. = parse_key_value(.message)
				+. = parse_key_value!(.message)
				"""#
		},
		{
			"title": "\(title) (assigning)"
			diff: #"""
				-. = parse_key_value(.message)
				+., err = parse_key_value(.message)
				"""#
		},
	]
}
