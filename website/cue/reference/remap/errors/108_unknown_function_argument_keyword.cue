package metadata

remap: errors: "108": {
	title:       "Unknown function argument keyword"
	description: """
		A [function call expression](\(urls.vrl_expressions)#regular-expression) passes an
		unknown named argument.
		"""
	resolution: """
		Correct the name to align with the documented argument names for the function.
		"""

	examples: [
		{
			"title": title
			source: #"""
				parse_timestamp(.timestamp, fmt: "%D")
				"""#
			diff: #"""
				-parse_timestamp(.timestamp)
				+parse_timestamp(.timestamp, format: "%D")
				"""#
		},
	]
}
