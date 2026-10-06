package metadata

remap: errors: "113": {
	title: "Non-string abort message"
	description: """
		The expression passed as an `abort` message does not resolve to a string.
		"""
	resolution: """
		Ensure the abort message expression resolves to a string, or use a
		[coercion function](\(urls.vrl_functions)/#coerce-functions) to convert it.
		"""

	examples: [
		{
			"title": "\(title)"
			source: #"""
				abort .
				"""#
			diff: #"""
				-abort .
				+abort string(.) ?? "abort"
				"""#
		},
	]
}
