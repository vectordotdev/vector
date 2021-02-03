package metadata

remap: errors: "108": {
	title:       "Unknown function argument keyword"
	description: """
		A [function call expression](\(urls.vrl_expressions)#\(remap.literals.regular_expression.anchor)) passes an
		unknown named argument.
		"""
	rationale:   null
	resolution: """
		Correct the name to align with the documented argument names for the function.
		"""

	examples: [
		{
			"title": title
			source: #"""
				parse_timestamp(.timestamp, fmt: "%D")
				"""#
			raises: compiletime: #"""
				error: \#(title)
				  ┌─ :1:1
				  │
				1 │ parse_timestamp(.timestamp, fmt: "%D")
				  │                             ^^^
				  │                             │
				  │                             The `fmt` argument is unknown
				  │
				"""#
			diff: #"""
				-parse_timestamp(.timestamp)
				+parse_timestamp(.timestamp, format: "%D")
				"""#
		},
	]
}
